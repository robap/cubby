# Presigned URLs, CopyObject & DeleteObjects — plan

**Spec:** [04-presigned-copy-batch-spec.md](04-presigned-copy-batch-spec.md) · **Roadmap:** Phase 4

## Approach

Three deliverables, two of them real handlers, one a verification. **Presigned
auth** is expected to already work — `s3s` + `SimpleAuth` (wired in
`http.rs` since Phase 1) validate query-string SigV4 the same as header SigV4 —
so its first step is a spike that drives a real presigned GET/PUT and only writes
code if a gap surfaces (*compatibility is proven, not claimed*). **CopyObject**
and **DeleteObjects** are two new `s3s::S3` trait methods on `Store`, each built
strictly on the Phase 1 primitives that already exist: copy reuses the
`.tmp/`→fsync→rename→row-last discipline (a copied object must be one real
browsable file — *the filesystem is the API too*), preserving the source row's
ETag verbatim so a copied multipart object keeps its `-N` composite (no re-hash);
batch delete reuses the row-first-then-unlink path in a single SQLite
transaction. No new tables, no schema change, no new dependency.

## Files

- `src/store.rs` — add `copy_object` and `delete_objects` to the `impl s3s::S3`
  block; add a small `stage_file_copy` helper (source file → `.tmp/` → fsync);
  extend the `s3s::dto` import list.
- `src/db.rs` — add a `delete_objects(bucket, &[key])` batch method that deletes
  all rows in one transaction (mirrors the existing single `delete_object`).
- `README.md` — document the two new ops, presigned-URL usage, and the
  presigned host-in-signature Docker gotcha (CONCEPT "known sharp edge").
- `tests/` — acceptance driven by boto3 / aws-sdk-js v3 / AWS CLI (per repo test
  convention established in Phases 1–3).

## Risks & unknowns

- **Presigned may need zero code.** Likely outcome; the spike confirms it. If
  `SimpleAuth` turns out not to validate query-string sigs, that becomes an
  extra step (close minimally) — but nothing downstream depends on the answer.
- **`s3s` `CopySource` is an enum** (`Bucket` / `AccessPoint` / `Outpost`). We
  handle only `Bucket`; the ARN variants → `NotImplemented` (or
  `InvalidRequest`). Path-style dev tool never sees them, but the match must be
  total.
- **Does `s3s` enforce the 1000-key DeleteObjects bound / parse `<Delete>` at
  the wire layer?** Confirm in the spike; if it already rejects, don't
  double-implement the guard. Same question for `MalformedXML` on a bad body.
- **Quiet-mode XML shape.** `DeleteObjectsOutput.deleted = None`/empty must
  render as a `DeleteResult` with no `<Deleted>` children — verify `s3s`
  serializes that (not a missing root).
- **Presigned host-in-signature** is inherent to SigV4 and *not* a bug to fix —
  only a README paragraph. Don't get lured into rewriting signatures.

## Steps

Each box ≈ one small commit moving an observable behavior. Check only when the
outcome is real, not when code is written.

- [x] **Presigned spike** — drive a boto3 `generate_presigned_url` GET against
      the running server with a credential-less HTTP client; confirm `200` +
      bytes. If it already passes, no handler code ships; if not, close the gap
      in `http.rs` minimally. Observable: a no-creds `curl` of the signed URL
      returns the object. **Result: passes with zero code** — no-creds `curl` of
      an `aws s3 presign` URL → `200` + `cmp`-clean bytes; tampered path → `403
      SignatureDoesNotMatch`; `--expires-in 1` after lapse → `403 AccessDenied`
      ("Request has expired"). `s3s` + `SimpleAuth` validate query-string SigV4;
      no handler change ships.
- [x] **CopyObject happy path (COPY directive, cross-key)** — implement
      `copy_object`: resolve `CopySource::Bucket`, load the source row (missing →
      handled next step), `stage_file_copy` the source file into `.tmp/` → fsync
      → rename into `buckets/<destb>/<destkey>` (parents created) → write the
      dest row **last**, carrying the source's `size`, **ETag verbatim**,
      `content_type`, and `metadata`, with `last_modified = now`. Return
      `CopyObjectResult { e_tag, last_modified }`. Observable: `aws s3api
      copy-object` / boto3 `copy_object` → `dst` GETs the source bytes,
      `cmp s3data/buckets/b/dst` clean, dest ETag == source ETag.
- [x] **CopyObject resolution errors** — source bucket missing →
      `NoSuchBucket`; source key missing → `NoSuchKey`; dest bucket missing →
      `NoSuchBucket`; non-`Bucket` copy-source (ARN) → `NotImplemented`. All
      checked before any bytes move. Observable: each bad `copy_object` returns
      the named code, no partial file left in the dest bucket dir.
- [x] **CopyObject `MetadataDirective: REPLACE`** — when REPLACE, take
      `content_type` (default `application/octet-stream`) and user `metadata`
      from the request instead of the source row; bytes still copied. Observable:
      copy with REPLACE + new content-type/metadata → `head_object(dst)` shows
      the request's values, source unchanged.
- [x] **CopyObject source==dest** — REPLACE with src key == dest key: skip the
      byte copy, rewrite only the row's `content_type`/`metadata` +
      `last_modified` (bytes untouched). COPY (default) with src==dest →
      `400 InvalidRequest` (S3's "illegal copy to itself"). Observable: the
      metadata-only update changes HEAD fields while `cmp s3data/buckets/b/k`
      still matches the original bytes; the self-COPY probe returns
      `InvalidRequest`.
- [x] **`db.delete_objects` batch method** — delete all given keys for a bucket
      in one transaction (idempotent per key, like `delete_object`). Observable:
      a unit/integration test deletes 3 rows in one call; `get_object` on each
      returns `None`.
- [x] **DeleteObjects handler** — implement `delete_objects`: bucket missing →
      `NoSuchBucket`; reject `> 1000` objects with `InvalidRequest` (skip if
      `s3s` already guards — confirm in the spike); batch-delete the rows via the
      new db method, then unlink each file (Phase 1 best-effort, `NotFound`
      ignored); return a `Deleted` entry per requested key. Observable: boto3
      `delete_objects` on `k1,k2,k3` → response lists all three,
      `Path(...k1)` etc. gone on disk, `list_objects_v2` returns none.
- [x] **DeleteObjects quiet mode** — `Quiet: true` → return an empty `deleted`
      list (and no `errors`); keys still removed. Observable: quiet batch
      response has no `<Deleted>` children yet the files are gone.
- [x] **Docs** — update `README.md`: add CopyObject + DeleteObjects to the
      supported-operations list, show presigned-URL generation/use, and add the
      one-paragraph presigned host-in-signature Docker gotcha (CONCEPT sharp
      edge). Create `README.md` leading with `./buckit serve` if it doesn't
      exist yet.

## Acceptance

Mirrors the spec. `/implement` isn't done until every box passes by driving the
named client against the running server.

### Presigned URLs
- [x] **boto3 presigned GET** — `generate_presigned_url("get_object")` fetched
      with a no-credentials HTTP client → `200` + exact bytes.
- [x] **boto3 presigned PUT** — `generate_presigned_url("put_object")`,
      credential-less `PUT` → `200`; authed `get_object` returns the bytes;
      `cmp s3data/buckets/b/<key>` clean.
- [x] **aws-sdk-js v3 presigned GET** — `getSignedUrl(GetObjectCommand)` +
      `fetch` (no creds) → `200` + bytes.
- [x] **AWS CLI presign** — `aws s3 presign s3://b/hello.txt` → `curl` of the URL
      returns the bytes, `200`.
- [x] **Expired presigned URL → 403** — `--expires-in 1` / `ExpiresIn=1`, fetched
      after lapse → `403 AccessDenied`.
- [x] **Tampered presigned URL → 403** — edited signed path/query →
      `403 SignatureDoesNotMatch`.

### CopyObject
- [x] **boto3 copy** — `copy_object(CopySource="b/src.bin", …Key="dst.bin")` →
      `200`; `dst` bytes == source; `cmp s3data/buckets/b/dst.bin src.bin` clean;
      dest ETag == source ETag.
- [x] **AWS CLI cross-bucket copy** — `aws s3 cp s3://b1/k s3://b2/k2` exits 0;
      `Path("s3data/buckets/b2/k2")` exists with the source bytes.
- [x] **Metadata directive COPY carries source metadata** — copy an object with
      `ContentType=application/json` + `{"team":"x"}` (default directive) →
      `head_object(dst)` shows both.
- [x] **source==dest REPLACE (metadata-only)** — REPLACE with new content-type +
      metadata on the same key → `head_object` reflects them,
      `cmp s3data/buckets/b/k` still matches original bytes.
- [x] **source==dest COPY → InvalidRequest** — self-copy with default directive →
      `400 InvalidRequest`.
- [x] **Copy source missing → NoSuchKey; source bucket missing → NoSuchBucket.**
- [x] **Copy of a multipart object preserves the composite ETag** — copy a
      Phase-3 multipart object → dest HEAD ETag == source `"<hex>-N"`, dest bytes
      `cmp` clean.

### DeleteObjects (batch)
- [x] **boto3 batch delete** — `delete_objects` on `k1,k2,k3` → `Deleted` lists
      all three; files gone on disk; `list_objects_v2` returns none.
- [x] **Batch delete is idempotent** — a never-existed key in the batch appears
      in `Deleted` (or does not error); request `200`.
- [x] **Quiet mode** — `Quiet:true` on all-valid keys → no `Deleted` entries, no
      `Errors`; keys removed from disk.
- [x] **`aws s3 rm --recursive`** — after putting several keys under a prefix,
      `aws s3 rm s3://b/prefix/ --recursive` exits 0 and the files are gone under
      `s3data/buckets/b/prefix/`.

## Progress notes

- **Presigned needed zero code**, as predicted. `s3s` + `SimpleAuth` validate
  query-string SigV4 (GET/PUT, expiry, tamper) with no handler change.
- **boto3 presigned PUT defaults to SigV2.** `generate_presigned_url("put_object")`
  emits a legacy `AWSAccessKeyId/Signature/Expires` (SigV2) URL unless the client
  is built with `Config(signature_version="s3v4")`; buckit (via `s3s`) validates
  SigV4 only, so a SigV2 URL is `403 SignatureDoesNotMatch`. This is standard for
  S3-compatible endpoints (MinIO documents the same) — not a buckit gap. The
  acceptance script pins `s3v4` for the PUT and the README calls it out. (GET
  happens to default to SigV4, so it worked unpinned.)
- **`s3s` does not enforce the 1000-key DeleteObjects bound** (its access-layer
  default is a no-op), so the handler guards it → `InvalidRequest`. `s3s` also
  serializes `deleted: None` as an empty `DeleteResult` correctly (quiet mode).
- **Acceptance is a shell script** (`tests/acceptance/presigned_copy_batch.sh`,
  mirroring `multipart.sh`) driving boto3, aws-sdk-js v3, and the AWS CLI — 19/19
  checks pass. aws-sdk-js v3 is bootstrapped via `npm install` into a temp dir
  (best-effort; skipped with a warning if node/npm are absent or offline).
