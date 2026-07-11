# Multipart & correct ETags — plan

**Status:** done · **Spec:** [03-multipart-etags-spec.md](03-multipart-etags-spec.md) · **Roadmap:** Phase 3

## Approach

Multipart reuses everything Phase 1 already built. A part is just a streamed
atomic write (the existing `stream_to_temp` discipline: hash incrementally,
`fsync`, never buffer the whole thing) landing at
`.multipart/<upload_id>/<part_number>` instead of `buckets/`. **Complete** is the
one genuinely new operation: validate the client's part list, stream-concatenate
the recorded part files into one temp file, `fsync`, atomically rename into
`buckets/`, then write the `objects` row — *after which a multipart object is
byte-for-byte an ordinary object*. This is the *filesystem-is-the-API-too* bet:
we assemble one real browsable file rather than serving a virtual concatenation,
so `cat`/`cmp`, Range GET, HEAD, and ListObjectsV2 all keep working unchanged.

SQLite stays the source of truth. Two new tables (`multipart_uploads`,
`multipart_parts`, per CONCEPT's schema sketch) hold in-flight state; part MD5s
are recorded at UploadPart so the **composite ETag** (`md5-of-md5s-N`) is
computed from stored hex digests with **no data re-read** — the CONCEPT
requirement and the correctness that *compatibility-is-proven* rides on (sync
tools compare this ETag). Following Phase 2's `listing.rs` pattern, the pure,
fiddly bits (composite-ETag formula, ETag normalization, part-list validation)
live in a new `multipart.rs` module unit-tested with no DB or server, and the
`s3s` trait methods in `store.rs` are thin adapters over the DB + that module.
Per-step TDD: unit tests for the pure module and DB methods, in-process
`aws-sdk-s3` integration tests for the handlers, and **boto3** + AWS CLI via
`/verify` for Acceptance.

## Files

- `src/multipart.rs` — **new.** Pure helpers: `composite_etag(part_md5_hex: &[&str])
  -> String` (`hex(md5(concat of raw 16-byte digests))` + `-N`); `normalize_etag`
  (strip quotes / `ETag::Strong|Weak` → inner hex) for comparing client-submitted
  part ETags; `validate_complete(submitted, recorded) -> Result<Vec<PartRef>,
  CompleteError>` (non-empty, strictly ascending, each exists + ETag matches);
  `new_upload_id()` (opaque, filesystem-safe token). Unit tests, incl. a known
  AWS composite-ETag vector.
- `src/db.rs` — schema additions (`multipart_uploads`, `multipart_parts`) +
  methods: `create_multipart`, `get_multipart` (→ bucket/key/content_type/
  metadata), `put_part` (INSERT OR REPLACE), `list_parts` (ascending, paged),
  `all_parts` (for Complete), `complete_multipart` (insert `objects` row + delete
  multipart rows in one txn), `delete_multipart` (abort). New `PartRow` /
  `MultipartRow` structs. Unit tests.
- `src/store.rs` — implement `create_multipart_upload`, `upload_part`,
  `list_parts`, `complete_multipart_upload`, `abort_multipart_upload`; add a
  `assemble_parts` helper (stream-concat part files → temp → fsync → rename)
  paralleling `stream_to_temp`.
- `src/lib.rs` — `mod multipart;`.
- `Cargo.toml` — add `getrandom = "0.2"` (already in the tree transitively) for
  `new_upload_id`; small, no toolchain impact.
- `tests/s3_api.rs` — integration tests driving `aws-sdk-s3` multipart.
- `tests/acceptance/multipart.sh` — live harness: boto3 100MB round-trip + AWS
  CLI lifecycle/field asserts against `buckit serve --port 0`.
- `README.md` — document the multipart operations + composite ETag (Docs step).

## Risks & unknowns

- **Composite ETag formula.** The classic bug: it is MD5 of the concatenated
  **raw 16-byte** part digests, *not* of the concatenated hex strings, and *not*
  MD5 of the whole assembled object. Suffix is `-<part count>`. A single part
  still gets `-1`. Pin it with a known AWS vector in a unit test or every sync
  tool re-transfers.
- **ETag normalization on Complete.** Client-submitted `CompletedPart.e_tag`
  arrives as `Option<ETag>` (Strong/Weak, quoted). Strip to inner hex before
  comparing to the stored value, or valid Completes spuriously 400.
- **Exact S3 error codes.** Empty part list → `InvalidRequest`; out-of-order →
  `InvalidPartOrder`; missing/mismatched part → `InvalidPart`; unknown
  `upload_id` → `NoSuchUpload`; create on missing bucket → `NoSuchBucket`;
  part number outside `1..=10000` → `InvalidArgument`. All confirmed present in
  `s3s` 0.14.1. Wrong codes make SDKs mis-handle failures.
- **Streaming assembly.** A 100MB Complete must stream part files through a fixed
  buffer into the temp file — never read a whole part (or the object) into
  memory. Same discipline as Phase 1's write path (*dev-tool startup/footprint*).
- **Crash windows.** Assemble→rename→row-insert mirrors Phase 1: a crash before
  the row insert leaves a harmless orphan file. The `objects`-insert + multipart-
  rows-delete run in one txn; a crash after that txn but before the staging tree
  is unlinked leaves an orphan `.multipart/<id>/` dir (invisible — no rows;
  sweepable, like `.tmp/`). Document, don't solve (sweep is v0.2).
- **upload_id safety.** Must be filesystem-safe (no separators) and collision-
  resistant; it names the staging subdir. Random hex from `getrandom`.
- **5MiB minimum deliberately not enforced** (spec open-question, adopted) so
  tiny-part tests can drive the full lifecycle. `EntityTooSmall` stays unused.
- **`s3s` completion future.** `CompleteMultipartUploadOutput.future` is left
  `None` — we complete synchronously (no 200-with-trailing-whitespace keepalive).

## Steps

Each box ≈ one small commit moving an observable behavior. Check only when the
outcome is real, not when code is written.

- [x] **Schema + DB layer** — add `multipart_uploads(upload_id PK, bucket, key,
      content_type, metadata, started_at)` and `multipart_parts(upload_id,
      part_number, size, etag, PK(upload_id,part_number))` to the schema, plus
      `create_multipart`, `get_multipart`, `put_part`, `all_parts`, `list_parts`
      (ascending, `after`/`limit` paged), `complete_multipart` (objects-insert +
      multipart-delete in one txn), `delete_multipart`. Unit tests: put/replace a
      part, list ascending + pagination, get returns captured content_type/
      metadata, complete removes multipart rows and creates the object row atomically.
- [x] **Pure `multipart.rs`** — `composite_etag`, `normalize_etag`,
      `validate_complete`, `new_upload_id`. Unit tests: composite ETag matches a
      **known AWS vector** (multi-part) and yields `-1` for one part; normalize
      strips quotes/Strong/Weak; validate rejects empty (`InvalidRequest`),
      out-of-order (`InvalidPartOrder`), missing + mismatched (`InvalidPart`),
      accepts an ascending subset; `new_upload_id` is non-empty and separator-free.
- [x] **CreateMultipartUpload** — allocates an `upload_id`, inserts the upload
      row (capturing `content_type` + `metadata`), creates `.multipart/<id>/`;
      returns `Bucket`/`Key`/`UploadId`. Missing bucket → `NoSuchBucket`.
      Integration test (`aws-sdk-s3`): create returns a non-empty upload id;
      create on absent bucket → `NoSuchBucket`.
- [x] **UploadPart** — streams the body to `.multipart/<id>/<n>` via the
      Phase-1 write discipline (incremental MD5, fsync), records `(size, md5hex)`,
      returns the part ETag (quoted hex MD5). Unknown `upload_id` →
      `NoSuchUpload`; part number outside `1..=10000` → `InvalidArgument`;
      re-uploading a part number replaces it. Integration test: two parts return
      hex-MD5 ETags; the part files exist on disk with the right bytes;
      re-upload of part 1 overwrites; bogus upload id → `NoSuchUpload`.
- [x] **ListParts** — returns recorded parts ascending with `PartNumber`, `Size`,
      `ETag`, `LastModified`, plus `Bucket`/`Key`/`UploadId`/`StorageClass`;
      `max-parts` default/cap 1000, `part-number-marker` resumes strictly after,
      `IsTruncated`/`NextPartNumberMarker` when cut short; unknown id →
      `NoSuchUpload`. Integration test: upload parts 1 & 2 (not completed),
      list shows both ascending with correct sizes/ETags; a `max-parts=1` page is
      truncated and its marker resumes at part 2.
- [x] **CompleteMultipartUpload** — validate via `validate_complete`, stream-
      assemble the selected parts in ascending order into `.tmp/` → fsync →
      rename into `buckets/<b>/<key>`, write the `objects` row (summed size,
      composite ETag, captured content_type/metadata, `last_modified=now`) and
      delete multipart rows in one txn, then unlink the staging tree. Returns
      `Bucket`/`Key`/`ETag`/`Location`. Integration test: 3-part upload →
      complete → returned ETag ends `-3`; `get_object` bytes equal
      `part1+part2+part3`; `head_object` size = sum; `object_path(...)` on disk
      `cmp`s clean; `.multipart/<id>/` is gone; completing to an existing key
      overwrites (last writer wins).
- [x] **AbortMultipartUpload** — deletes multipart rows and the
      `.multipart/<id>/` tree; no object row created. Unknown id →
      `NoSuchUpload`. Integration test: abort a live upload → subsequent
      `list_parts` → `NoSuchUpload`, `head_object` → 404, staging dir gone.
- [x] **Complete error-path sweep** — `InvalidPart` (wrong/missing part ETag),
      `InvalidPartOrder` (descending list), `InvalidRequest` (empty list),
      `NoSuchUpload` (bogus id on complete). Integration tests for each (some
      already covered above; fill gaps). Confirms exact wire error codes via
      `aws-sdk-s3` error inspection.
- [x] **Docs** — `README.md` gains a "Multipart (Phase 3)" section (Create/
      UploadPart/Complete/Abort/ListParts, composite `md5-of-md5s-N` ETag,
      relaxed 5MiB-minimum, boto3 auto-multipart at 8MB); remove multipart from
      any "Not yet implemented" list.

## Acceptance

Mirrors the spec. `/implement` isn't done until every box passes by driving the
named client — **boto3** (forces multipart at 8MB) + the **AWS CLI** for
per-verb field asserts — against a live `buckit serve --port 0`, via a harness
`tests/acceptance/multipart.sh` (paralleling `listing.sh`).

- [x] **boto3 100MB round-trip** — `upload_file` of a 100MB file with default
      `TransferConfig` (auto-multipart) succeeds; `download_file` back matches
      the source `hashlib.md5`; `cmp s3data/buckets/b/big.bin` against the source
      is clean (one real assembled file).
- [x] **Composite ETag matches the formula** — `head_object` returns
      `"<32 hex>-<N>"`; an independent script computing
      `md5(b"".join(md5(part_i).digest())).hexdigest() + f"-{N}"` equals it
      (quotes stripped).
- [x] **Explicit low-level lifecycle (boto3)** — `create_multipart_upload` →
      three `upload_part` (hex-MD5 ETags) → `complete_multipart_upload` returns
      the composite ETag; `get_object` == `part1+part2+part3`;
      `s3data/.multipart/<upload_id>/` no longer exists.
- [x] **AWS CLI large upload** — `aws s3 cp big.bin s3://b/cli.bin` (auto-
      multipart > 8MB) exits 0; `aws s3 cp s3://b/cli.bin` back round-trips;
      `aws s3api head-object` shows a `-N` ETag.
- [x] **ListParts (AWS CLI)** — after uploading parts 1 & 2 (not completed),
      `aws s3api list-parts` lists exactly parts 1 & 2 ascending with correct
      `Size` and hex ETag.
- [x] **Abort (AWS CLI)** — `aws s3api abort-multipart-upload` → staging dir
      gone; following `list-parts` → `NoSuchUpload`; `head-object` → 404.
- [x] **Complete error paths** — (a) wrong part ETag → `InvalidPart`; (b) parts
      out of ascending order → `InvalidPartOrder`; (c) bogus `--upload-id` on
      upload/complete/list → `NoSuchUpload`; (d) create on absent bucket →
      `NoSuchBucket`.
- [x] **Overwrite (boto3)** — single-PUT object at `k`, then complete a multipart
      to `k`; `get_object` returns the multipart bytes + composite `-N` ETag;
      `cmp s3data/buckets/b/k` matches the multipart content.
- [x] **Range GET on assembled object (boto3)** —
      `get_object(Range="bytes=8388600-8388700")` returns exactly those 101 bytes.
- [x] **Listing shows the composite ETag (AWS CLI)** — `aws s3api
      list-objects-v2 --bucket b` includes the completed object with its
      `"<hex>-N"` ETag.

## Progress notes

- **ListParts `LastModified`.** The `multipart_parts` schema (per CONCEPT's
  sketch) records no per-part timestamp, so ListParts reports the current time
  for every part's `LastModified` rather than the moment it was uploaded. All
  acceptance/integration asserts key off `PartNumber`/`Size`/`ETag`; the field
  is present and well-formed, which is all any matrix client reads. Add a
  `uploaded_at` column later only if a client is shown to depend on it.
- **Acceptance harness boto3 bootstrap.** `tests/acceptance/multipart.sh` needs
  boto3; when it isn't importable the script provisions it into a throwaway `uv`
  venv (falls back with a clear message if `uv` is absent), keeping the harness
  self-contained like `listing.sh`. Verified end-to-end: 18/18 checks pass, incl.
  the 100MB round-trip (boto3 split it into 13 parts → composite `…-13`).
