# Core object I/O — spec

**Status:** done · **Roadmap:** Phase 1 · **Slug:** 01-core-object-io

## Why

Nothing works until a real S3 client can create a bucket and round-trip an
object's bytes. This phase wires the `s3s` protocol crate to a
filesystem+SQLite backend and proves the **storage model** — atomic streaming
writes, SQLite as source of truth, crash-safe ordering — that every later
phase sits on. It's the foundation, so it comes first and must be correct
before listing/multipart/UI build on it.

North stars served:
- **The filesystem is the API too** — a `PUT` lands as a real, `cat`-able file
  at a derived path under `buckets/`.
- **Starts in milliseconds, zero config** — `cubby serve ./s3data` bootstraps
  the data dir and is ready with default credentials, no setup.
- **Compatibility is proven** — the AWS CLI, driving one operation at a time,
  is the acceptance client.

## In scope

- **`serve` CLI + bootstrap.** `cubby serve <dir>` creates `<dir>` with a
  self-`.gitignore` (`*`), `buckets/`, `.tmp/`, `.multipart/`, and
  `meta.sqlite` (WAL). Prints the startup banner (S3 API URL, access/secret,
  data dir). Flags: `--bind` (default `127.0.0.1`), `--port` (default `9000`,
  `0` = ephemeral, printed machine-parseably), credential overrides.
- **SQLite schema v0** — `buckets` and `objects` tables per CONCEPT (objects
  `WITHOUT ROWID`, `PK(bucket,key)`), WAL mode.
- **Single-port routing skeleton** — SigV4-authed / S3 requests → `s3s`
  handler; `/_/` prefix → a placeholder handler (real UI is Phase 5); bare
  `GET /` → ListBuckets XML.
- **Header SigV4 auth** via `s3s`, validated against configured credentials
  (default `local`/`localsecret`, overridable by flag/env).
- **Bucket ops:** CreateBucket (accept + ignore any region/location
  constraint), DeleteBucket (only when empty), ListBuckets, HeadBucket.
- **Object ops:** PutObject, GetObject (incl. single `Range`), HeadObject,
  DeleteObject.
- **Content-MD5 ETag** for single-part objects (hex MD5 of the body), computed
  streaming during the write.
- **Content-Type and user metadata** (`x-amz-meta-*`) stored and returned on
  HEAD/GET.
- **Key→path derivation** — filesystem path is *derived* from the canonical
  key (percent-encode the Windows-illegal set `<>:"|?*`, trailing dots/spaces,
  reserved names); nested key prefixes create nested directories; filenames
  are never decoded back into keys.
- **Streaming atomic write path** — `.tmp/` → incremental hash → fsync →
  rename into `buckets/…` → SQLite insert. Delete = SQLite row first, then
  unlink. (Crash between rename and insert leaves a harmless orphan file.)
- **Correct error codes** for the above verbs: `NoSuchBucket`, `NoSuchKey`,
  `BucketAlreadyOwnedByYou`/`BucketAlreadyExists`, `BucketNotEmpty`,
  `SignatureDoesNotMatch`.

## Out of scope

- **ListObjectsV2 / ListObjects v1** → Phase 2. (Consequence: acceptance uses
  `aws s3api` low-level object ops, not `aws s3 ls s3://bucket`.)
- **Multipart** and composite `md5-of-md5s-N` ETags → Phase 3.
- **Presigned/query-string auth, CopyObject, DeleteObjects batch** → Phase 4.
- **Web UI + live request-log/SSE** → Phase 5; `/_/` is a placeholder here.
- **`--seed`** → Phase 6.
- **`--accept-any` credentials mode** → fast-follow; Phase 1 uses fixed
  default (overridable) credentials.
- **The rich per-request event log / `--quiet`** → Phase 5 (only the startup
  banner is in scope now).
- CONCEPT non-goals (versioning, tagging, IAM, …).

## Behavior

- **Addressing:** path-style only (`http://127.0.0.1:9000/<bucket>/<key>`).
  Virtual-host is a later trick.
- **CreateBucket:** idempotent for the same owner (returns
  `BucketAlreadyOwnedByYou` on re-create); any region accepted and ignored.
- **DeleteBucket:** succeeds only if the bucket has no objects, else
  `BucketNotEmpty`; on success the `buckets/<name>/` directory is removed.
- **PutObject:** body streamed to a temp file in `.tmp/`, MD5 hashed
  incrementally (never buffering the whole object), fsync'd, atomically
  renamed to its derived path, then the `objects` row is inserted/replaced.
  Overwriting an existing key replaces bytes and metadata. Response `ETag` is
  the quoted hex MD5.
- **GetObject:** streams bytes from disk. With a `Range: bytes=a-b` header,
  returns `206 Partial Content`, a `Content-Range`, and exactly the requested
  slice. Returns `Content-Type`, `ETag`, `Last-Modified`.
- **HeadObject:** same headers as GET, no body. Missing key → `404 NoSuchKey`.
- **DeleteObject:** removes the SQLite row, then unlinks the file. Idempotent
  (deleting a missing key still returns success, per S3). A subsequent GET/HEAD
  → `NoSuchKey`.
- **Durability ordering is observable:** after a successful PUT the bytes exist
  at the derived path *and* a row exists; the row is the source of truth for
  existence (an orphan file with no row reads as "does not exist").
- **Illegal-char keys:** a key containing `<>:"|?*` or trailing dot/space is
  stored under a percent-encoded filename but reported back to clients by its
  original canonical key.
- **Auth:** requests must carry a valid header SigV4 signature for the
  configured credentials; a bad secret → `403 SignatureDoesNotMatch`.

## Acceptance criteria

Named client is the **AWS CLI** (low-level `s3api` for 1:1 op mapping), plus
filesystem assertions. Each becomes a checkbox in the plan.

- [ ] `cubby serve ./s3data` prints the banner and creates `./s3data` with
      `.gitignore` containing `*`, plus `buckets/`, `.tmp/`, `.multipart/`,
      `meta.sqlite`.
- [ ] `aws s3api create-bucket --bucket uploads` → 200; `s3data/buckets/uploads/`
      exists.
- [ ] `aws s3api list-buckets` → JSON lists `uploads`.
- [ ] `aws s3api head-bucket --bucket uploads` → 200; `head-bucket` on a missing
      bucket → 404 `NoSuchBucket`.
- [ ] `aws s3api put-object --bucket uploads --key report.pdf --body report.pdf`
      → 200; `cmp s3data/buckets/uploads/report.pdf report.pdf` is clean (real
      bytes on disk); returned `ETag` == hex MD5 of `report.pdf`.
- [ ] `aws s3api put-object --key photos/cat.jpg …` → bytes land at
      `s3data/buckets/uploads/photos/cat.jpg` (nested dirs created).
- [ ] `aws s3api get-object --bucket uploads --key report.pdf out.pdf` →
      `cmp out.pdf report.pdf` clean.
- [ ] `aws s3api get-object --range bytes=0-99 … out.part` → HTTP 206, `out.part`
      is exactly 100 bytes and equals the first 100 bytes of the source.
- [ ] `aws s3api head-object --bucket uploads --key report.pdf` → correct
      `ContentLength`, `ETag`, `ContentType`, `LastModified`.
- [ ] `aws s3api put-object --metadata team=infra …` then `head-object` →
      `Metadata` shows `{"team":"infra"}`.
- [ ] `aws s3api put-object --key 'weird:name?.txt' …` round-trips via the SDK;
      on-disk filename is percent-encoded (no raw `:`/`?`); `head-object` still
      reports key `weird:name?.txt`.
- [ ] `aws s3api delete-object --bucket uploads --key report.pdf` → 200;
      `s3data/buckets/uploads/report.pdf` is gone; subsequent `get-object` →
      404 `NoSuchKey`.
- [ ] `aws s3api delete-bucket` on non-empty `uploads` → `BucketNotEmpty`; after
      emptying, `delete-bucket` → 200 and `s3data/buckets/uploads/` is gone.
- [ ] AWS CLI configured with a wrong secret key → any request returns 403
      `SignatureDoesNotMatch`.

## Open questions

Resolved by adopting the proposed defaults when planning began (revisit any if
you disagree):

- **Credentials scope.** ✅ Fixed default `local`/`localsecret`, overridable via
  `--access-key`/`--secret-key` + env. `--accept-any` deferred to a fast-follow.
- **`/_/` placeholder.** ✅ `/_/*` returns a minimal `501` "UI coming in Phase 5"
  until the web UI lands.
- **Request log in Phase 1.** ✅ Startup banner only; the full event system +
  `--quiet` are Phase 5.
