# Multipart & correct ETags — spec

**Status:** done · **Roadmap:** Phase 3 · **Slug:** 03-multipart-etags

## Why

boto3 — the single most common S3 client in app development — **auto-switches
to multipart upload at 8MB** (`TransferConfig` default). So does the AWS CLI and
every sync tool for large files. A store that only does single-shot `PutObject`
silently fails the moment someone uploads a real file. Multipart is not an
advanced feature; it is table stakes for "boto3 can upload."

The second half is the **composite ETag**. A multipart object's ETag is not the
MD5 of its bytes — it is `md5-of-md5s-N`: the hex MD5 of the concatenated raw
MD5 digests of each part, suffixed with `-<part count>`. Sync tools (rclone,
`aws s3 sync`) compare ETags to decide whether to re-transfer; a wrong ETag
means endless re-uploads or skipped changes. It must match byte-for-byte what
real S3 produces, and — per CONCEPT — be computed from **recorded part MD5s
without re-reading the data**.

North stars served:
- **Compatibility is proven, not claimed** — the acceptance is boto3
  round-tripping a 100MB object (which forces the multipart path), not a
  hand-rolled request.
- **The filesystem is the API too** — a completed multipart object is a single
  **real assembled file** at `buckets/<b>/<key>`, indistinguishable from a
  single-PUT object. `cat`/`cmp` works; Range GET works; it lists normally.
- **Dev tool first** — in-flight parts are browsable real files under
  `.multipart/<upload_id>/`, and the 5MiB-minimum-part rule is relaxed so tests
  can drive the full lifecycle with tiny parts.

## In scope

The five-verb multipart lifecycle, implemented against the `s3s` trait:

- **CreateMultipartUpload** (`POST /<bucket>/<key>?uploads`) — allocate an
  opaque `upload_id`, capturing `content_type` and user `metadata` for the
  eventual object. Returns `Bucket`, `Key`, `UploadId`.
- **UploadPart** (`PUT /<bucket>/<key>?partNumber=N&uploadId=…`) — stream one
  part's body to `.multipart/<upload_id>/<N>`, hashing MD5 incrementally (same
  streaming/fsync path as Phase 1's `PutObject`). Returns the part's ETag
  (quoted hex MD5). Re-uploading the same part number replaces it.
- **CompleteMultipartUpload** (`POST /<bucket>/<key>?uploadId=…`) — validate the
  client's part list against recorded parts, **assemble** the parts into one
  final object file (streamed concat → fsync → atomic rename into `buckets/`),
  write the object row with the **composite ETag**, and clean up staging.
  Returns `Bucket`, `Key`, `ETag` (composite), `Location`.
- **AbortMultipartUpload** (`DELETE /<bucket>/<key>?uploadId=…`) — discard the
  upload: delete part rows and the `.multipart/<upload_id>/` tree. No object is
  created. Idempotent-ish per S3 (a live upload_id → success; an already-gone
  one → `NoSuchUpload`).
- **ListParts** (`GET /<bucket>/<key>?uploadId=…`) — list uploaded parts in
  ascending part-number order with `PartNumber`, `Size`, `ETag`, `LastModified`;
  supports `max-parts` / `part-number-marker` pagination.

Storage:
- New SQLite tables `multipart_uploads` and `multipart_parts` (CONCEPT schema
  sketch). Part MD5s recorded so the composite ETag needs no data re-read.
- Staging under `.multipart/<upload_id>/<part_number>`, on the same filesystem
  as `buckets/` so assembly and the final rename stay cheap/atomic.

Correctness:
- **Composite ETag** = `hex(md5(concat_of_raw_part_md5_digests))` + `-` +
  `<part count>`, stored verbatim in the `objects.etag` column and returned by
  GET/HEAD/List exactly like a single-PUT ETag.
- The assembled object is a normal Phase 1/2 object afterward: Range GET, HEAD,
  ListObjectsV2/v1, and Delete all work unchanged and see the composite ETag.

## Out of scope

- **ListMultipartUploads** (enumerate in-flight uploads bucket-wide). Not in the
  CONCEPT MVP verb list; the happy path doesn't need it. Left as the `s3s`
  default (`NotImplemented`).
- **UploadPartCopy** (part sourced from an existing object). Copy semantics are
  Phase 4; left `NotImplemented`.
- **Presigned/query-string auth for multipart** → Phase 4. Multipart here is
  header-SigV4 authed like Phases 1–2.
- **Sweeping abandoned uploads** (parts uploaded but never completed/aborted).
  No TTL/GC in this phase; a `reindex`/sweep is a v0.2 candidate. Orphaned
  `.multipart/` dirs are harmless and sweepable, like Phase 1 `.tmp/` orphans.
- **Checksums (CRC32/SHA), SSE/KMS, ACLs, object-lock, tagging, `mpu-object-size`,
  `if-match`/`if-none-match` on Complete** — parsed by `s3s`, accepted and
  ignored (CONCEPT non-goals).
- **Enforcing the 5MiB minimum part size.** Real S3 returns `EntityTooSmall`
  when a non-final part is < 5MiB; we deliberately relax this (dev-tool
  ergonomics — small-part tests). See open questions.

## Behavior

- **Addressing:** path-style only, consistent with Phases 1–2.
- **upload_id:** an opaque, filesystem-safe token (clients must not parse it).
  Unique per CreateMultipartUpload; concurrent uploads to the same key get
  distinct ids and don't interfere. It also serves as the `.multipart/`
  subdirectory name, so it must contain no path separators.
- **Bucket must exist:** CreateMultipartUpload on a missing bucket →
  `404 NoSuchBucket`. (Parts/Complete/Abort/ListParts key off `upload_id`, whose
  existence implies the bucket existed at create time.)
- **Unknown upload_id:** UploadPart, Complete, Abort, or ListParts against an
  `upload_id` that doesn't exist (never created, or already completed/aborted) →
  `404 NoSuchUpload`.
- **Part numbers:** accepted range `1..=10000` (S3's bound); outside → `400
  InvalidArgument`. Parts may be uploaded in any order and need not be
  contiguous (1, 5, 9 is legal). Re-uploading an existing part number overwrites
  its bytes, size, and ETag (last write wins).
- **UploadPart ETag:** quoted hex MD5 of that part's bytes, the value the client
  echoes back in the Complete part list.
- **Complete — validation (all before any assembly):**
  - The submitted part list must be **non-empty** → else `400 InvalidRequest`.
  - Parts must be in **strictly ascending `PartNumber`** order → else `400
    InvalidPartOrder`.
  - Every submitted part must **exist** among the recorded parts and its
    submitted `ETag` must **match** the recorded one → else `400 InvalidPart`
    (naming the offending part number). ETag comparison ignores surrounding
    quotes.
  - Parts not listed by the client are dropped (S3 lets you complete with a
    subset, in ascending order).
- **Complete — assembly:** concatenate the selected parts' bytes **in ascending
  part-number order** into a temp file in `.tmp/`, `fsync`, then atomically
  rename into `buckets/<b>/<key>` (creating parent dirs). The object row is
  written **last** (source of truth), carrying: total size (sum of part sizes),
  the composite ETag, the `content_type`/`metadata` captured at Create,
  `last_modified = now`. A crash between rename and row-insert leaves a harmless
  orphan file, exactly like Phase 1. After the row lands, delete the part rows
  and the `.multipart/<upload_id>/` tree.
- **Composite ETag:** `hex(md5( md5(part_1) ‖ md5(part_2) ‖ … ))` where each
  `md5(part_i)` is the **raw 16-byte** digest (recorded at UploadPart, decoded
  from the stored hex — never recomputed from bytes), concatenated in ascending
  part order, then MD5'd; the hex result is suffixed `-<N>` with `N` = number of
  completed parts. A single-part multipart still yields the `-1` suffix (this is
  how real S3 behaves and how sync tools tell multipart objects apart).
- **Overwrite:** completing to a key that already holds an object (single-PUT or
  a prior multipart) replaces it — GET afterward returns the new bytes and the
  new composite ETag (last writer wins), same `INSERT OR REPLACE` as Phase 1.
- **Abort:** removes part rows and the staging tree; a subsequent ListParts or
  Complete on that `upload_id` → `NoSuchUpload`. No object row is created.
- **ListParts:** returns parts in ascending `PartNumber` with `Size`, `ETag`
  (quoted hex MD5), `LastModified`, plus `Bucket`, `Key`, `UploadId`,
  `StorageClass=STANDARD`. `max-parts` defaults to 1000 and caps at 1000;
  `part-number-marker` resumes strictly after the given part number;
  `IsTruncated`/`NextPartNumberMarker` set when a page is cut short.
- **Interaction with listing (Phase 2):** an in-flight multipart upload does
  **not** appear in ListObjectsV2/v1 (no object row until Complete). A completed
  one appears as an ordinary object carrying its composite `-N` ETag.
- **Consistency:** as everywhere, SQLite is the source of truth — a part or
  object is "there" only when its row exists; bytes without a row are invisible
  orphans.

## Acceptance criteria

Named client is **boto3** (the client that forces multipart at 8MB), with the
**AWS CLI** (`aws s3api`) for precise per-verb field assertions and independent
`hashlib`/`cmp` checks for ETag and byte correctness. Each becomes a checkbox in
the plan.

- [ ] **boto3 100MB round-trip (the phase's headline).** `boto3` `upload_file`
      of a 100MB file to `s3://b/big.bin` using the **default** `TransferConfig`
      (auto-multipart) → succeeds. `download_file` back yields a file whose
      `hashlib.md5` equals the original's. `cmp` of
      `s3data/buckets/b/big.bin` against the source file is clean (filesystem
      assertion — it's one real assembled file).
- [ ] **Composite ETag matches real S3's formula.** After the boto3 upload above
      (or a controlled N-part upload), `head_object` returns an ETag of the form
      `"<32 hex>-<N>"`. A script that reads the parts, computes
      `md5(b"".join(md5(part_i).digest() for i in parts)).hexdigest() + f"-{N}"`
      independently equals the returned ETag (quotes stripped).
- [ ] **Explicit low-level lifecycle (boto3).** `create_multipart_upload` →
      three `upload_part` calls (numbers 1,2,3, each returning a hex-MD5 ETag) →
      `complete_multipart_upload` with those parts → returns the composite ETag;
      `get_object` bytes equal `part1+part2+part3` concatenated; the staging dir
      `s3data/.multipart/<upload_id>/` no longer exists (filesystem assertion).
- [ ] **AWS CLI large upload.** `aws s3 cp big.bin s3://b/cli.bin` (CLI
      auto-multiparts files > 8MB) exits 0; `aws s3 cp s3://b/cli.bin -` (or to a
      file) round-trips the bytes; `aws s3api head-object` shows a `-N` ETag.
- [ ] **ListParts (AWS CLI).** After uploading parts 1 and 2 (not yet
      completed), `aws s3api list-parts --bucket b --key k --upload-id <id>`
      lists exactly parts 1 and 2 in ascending order with correct `Size` and hex
      ETag for each.
- [ ] **Abort (AWS CLI).** `aws s3api abort-multipart-upload` on a live upload →
      the `s3data/.multipart/<upload_id>/` directory is gone (filesystem
      assertion), a following `aws s3api list-parts` → `NoSuchUpload`, and
      `aws s3api head-object --bucket b --key k` → `404` (no object was created).
- [ ] **Complete error paths (AWS CLI / boto3).**
      (a) Complete with a part ETag that doesn't match the uploaded one →
      `InvalidPart`.
      (b) Complete with parts listed out of ascending order → `InvalidPartOrder`.
      (c) `upload-part` / `complete` / `list-parts` with a bogus `--upload-id` →
      `NoSuchUpload`.
      (d) `create-multipart-upload` on a nonexistent bucket → `NoSuchBucket`.
- [ ] **Overwrite (boto3).** PUT a small single-shot object at `s3://b/k`, then
      complete a multipart upload to the same `k`; `get_object` returns the
      multipart bytes and the composite `-N` ETag (last writer wins);
      `cmp s3data/buckets/b/k` matches the multipart content.
- [ ] **Range GET on assembled object (boto3).** `get_object(Bucket=b, Key=k,
      Range="bytes=8388600-8388700")` on a completed multipart object returns
      exactly those 101 bytes (proves the assembled object is a normal
      Range-capable file, Phase 1 path).
- [ ] **Listing shows the composite ETag (AWS CLI).** `aws s3api
      list-objects-v2 --bucket b` includes the completed multipart object with
      its `"<hex>-N"` ETag (confirms the Phase 2 note: multipart objects list
      correctly once their ETags are stored).

## Open questions

Proposed defaults below; flagged for review, not blocking the criteria. Adopt
unless you disagree.

- **5MiB minimum part size.** ✅ **Do not enforce.** Real S3 rejects a non-final
  part < 5MiB with `EntityTooSmall`, but enforcing it would make the whole
  lifecycle untestable with tiny parts and buys nothing for a dev tool. Revisit
  only if a real client depends on the rejection (none in the matrix do).
- **upload_id generation.** ✅ Opaque random token (e.g. hex of random bytes),
  filesystem-safe (no separators), unguessable enough to avoid collisions.
  Clients treat it as opaque. Plan picks the exact source (`getrandom` vs.
  counter+time+pid); no new heavy dependency required.
- **Assemble-by-copy vs. keep-parts-and-stitch-on-read.** ✅ **Assemble by copy**
  at Complete into one real file. CONCEPT's "filesystem is the API" demands a
  single browsable object; serving a virtual concatenation would break `cat`,
  `cmp`, and direct-file test assertions. The extra one-time copy is acceptable
  for a dev tool.
- **ListMultipartUploads.** ✅ Out of scope (left `NotImplemented`). Not in the
  MVP verb list and not on the happy path; promote later only if a matrix client
  calls it during cleanup.
- **Part-number range validation.** ✅ Enforce `1..=10000` (S3's bound); outside
  → `InvalidArgument`. Cheap and keeps us honest against clients that probe it.
