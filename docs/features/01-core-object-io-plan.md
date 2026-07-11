# Core object I/O — plan

**Status:** done · **Spec:** [01-core-object-io-spec.md](01-core-object-io-spec.md) · **Roadmap:** Phase 1

## Approach

A single binary crate `buckit`. `main` parses CLI (`clap`), bootstraps the data
dir, opens `meta.sqlite` (WAL), and starts a `hyper` server whose top layer
routes: `/_/*` → 501 stub, and everything else → the `s3s` service. `s3s` owns
the wire protocol, header SigV4, and XML; we implement its `S3` trait against a
`Store` that pairs SQLite (source of truth for existence/metadata) with the
filesystem (bytes). Objects are written **streaming-atomic**: temp file in
`.tmp/` → incremental MD5 → fsync → rename into `buckets/…` → SQLite insert —
serving *filesystem-is-the-API* (real `cat`-able files) and crash-safety (row is
authoritative; a rename-without-insert is a harmless orphan). The on-disk path
is *derived* from the canonical key and never decoded back. rusqlite is sync, so
DB work runs behind `spawn_blocking` over a single `Mutex<Connection>` with
`busy_timeout` (fine for Phase 1's low concurrency; see Risks).

Per-step TDD uses unit tests for pure logic (key→path) and in-process
integration tests (spawn the server on `--port 0`, drive it with `aws-sdk-s3`)
for handlers. The spec's named end-to-end verifier is the **AWS CLI**, run via
`/verify` for the Acceptance boxes.

## Files

- `Cargo.toml` — deps: `s3s`, `tokio`, `hyper`/`hyper-util`, `rusqlite`
  (bundled), `md-5`, `clap`, `percent-encoding`, `time`, `thiserror`/`anyhow`,
  `tracing`; dev-deps: `aws-sdk-s3`, `aws-config`, `tempfile`.
- `src/main.rs` — runtime, wire CLI → datadir → store → server.
- `src/cli.rs` — `clap` args (`serve <dir>`, `--bind`, `--port`,
  `--access-key`, `--secret-key`).
- `src/datadir.rs` — layout paths + bootstrap (`.gitignore`, `buckets/`,
  `.tmp/`, `.multipart/`).
- `src/db.rs` — connection open, schema v0, WAL/pragmas, bucket/object queries.
- `src/keypath.rs` — canonical-key → filesystem-path derivation (encode only).
- `src/store.rs` — the `S3` trait impl tying `db` + fs together.
- `src/http.rs` — hyper server + routing layer (`/_/` stub, s3s service, bare
  `GET /`).
- `src/banner.rs` — startup banner.
- `README.md` — created in the Docs step.

## Risks & unknowns

- **`s3s` API specifics** (biggest unknown): exact trait signatures, how the
  streaming request/response body and parsed `Range` are exposed, and how to
  install fixed-credential SigV4 auth. Pin a version and read its docs/examples
  (and `s3s-fs`) before step 3.
- **AWS CLI streaming signature:** `put-object` may send
  `x-amz-content-sha256: STREAMING-AWS4-HMAC-SHA256-PAYLOAD` +
  `Content-Encoding: aws-chunked`. `s3s` should de-chunk and verify; confirm
  our body reader sees decoded bytes, not chunk framing.
- **Atomic rename requires same filesystem:** `.tmp/` lives inside the data dir,
  so rename into `buckets/` is same-FS/atomic — hold this invariant.
- **SQLite concurrency** (ROADMAP open question): single `Mutex<Connection>` +
  `busy_timeout` is enough for Phase 1; a shared instance under real parallelism
  is revisited later.
- **Windows-illegal-name rule** must be exact: encode `<>:"|?*`, trailing
  dots/spaces, and reserved device names — unit-test the corners.

## Steps

Each box ≈ one small commit moving an observable behavior. Check only when the
outcome is real, not when code is written.

- [x] **Scaffold crate** — `cargo build` succeeds; `buckit --help` and
      `buckit serve --help` list the flags.
- [x] **Data-dir bootstrap** — `buckit serve ./s3data` creates `./s3data` with
      `.gitignore` (`*`), `buckets/`, `.tmp/`, `.multipart/`, and an (empty)
      `meta.sqlite`; re-running is idempotent.
- [x] **SQLite schema v0** — `meta.sqlite` opens in WAL with `buckets` and
      `objects` (`WITHOUT ROWID`, `PK(bucket,key)`) tables; `PRAGMA
      journal_mode` = `wal`. (source-of-truth model)
- [x] **Server up + routing + SigV4** — hyper serves on `--bind`/`--port`
      (`0` = ephemeral, printed); `/_/*` → 501 stub; S3 requests hit the `s3s`
      service with fixed-cred auth. `aws s3api list-buckets` → 200 empty list;
      wrong secret → 403 `SignatureDoesNotMatch`. Banner prints.
- [x] **Key→path derivation** (pure, unit-tested) — encode `<>:"|?*`, trailing
      dots/spaces, reserved names; nested prefixes → nested dirs; no
      decode-back path. Unit tests green. (*filesystem-is-the-API*)
- [x] **CreateBucket + ListBuckets** — `create-bucket` makes
      `buckets/<name>/` + row; `list-buckets` returns it; re-create →
      `BucketAlreadyOwnedByYou`; any region accepted/ignored.
- [x] **HeadBucket + DeleteBucket** — `head-bucket` 200 / 404 `NoSuchBucket`;
      `delete-bucket` empty → 200 + dir removed; non-empty → `BucketNotEmpty`.
- [x] **PutObject (streaming atomic write)** — body → `.tmp/` → incremental
      MD5 → fsync → rename → row insert (size, etag, content-type, metadata,
      last-modified). `put-object` → on-disk bytes `cmp`-clean, `ETag` = quoted
      hex MD5; nested key creates dirs; illegal-char key stored percent-encoded.
- [x] **HeadObject** — returns `ContentLength`, `ETag`, `ContentType`,
      `LastModified`, and `Metadata`; missing key → `NoSuchKey`.
- [x] **GetObject (full)** — streams bytes from disk; `get-object` output
      `cmp`-clean against source; correct headers.
- [x] **GetObject Range** — `--range bytes=0-99` → HTTP 206 + `Content-Range`,
      exactly the requested slice.
- [x] **DeleteObject** — row deleted, then file unlinked; idempotent on missing
      key; subsequent GET/HEAD → `NoSuchKey`.
- [x] **Error-code sweep** — confirm every negative path returns the S3 code the
      spec lists (a consolidation pass over prior boxes; add tests for any gap).
- [x] **Docs** — create `README.md` leading with `./buckit serve`; document the
      `serve` flags, default credentials, supported bucket/object operations,
      and path-style-only addressing.

## Progress notes

- **HeadBucket missing → 404 `NotFound`, not `NoSuchBucket` code.** HeadBucket
  responses carry no body, so the `NoSuchBucket` error *code* can't be
  transmitted; S3/`s3s` return a bodyless 404 and the aws-sdk models it as
  `NotFound`. Tests assert the 404 (`is_not_found()`); the spec's "404
  NoSuchBucket" wording holds on status. DeleteBucket (which has a body) does
  return the `NoSuchBucket` code.
- **Integration tests drive the `aws-sdk-s3` Rust client**, not the `aws` CLI
  (not installed in this env). Same wire protocol / SigV4 path; the Acceptance
  boxes still call for a CLI run via `/verify`.
- **Error-code sweep refinements.** GET/HEAD on a *missing bucket* now return
  `NoSuchBucket` (not `NoSuchKey`) via a bucket-existence check on the miss
  path. `BucketAlreadyExists` is unreachable in Phase 1 (single owner) — a
  re-create by the same owner is always `BucketAlreadyOwnedByYou`, per the
  spec's Behavior section.

## Acceptance

Mirrors the spec. All boxes verified by driving the real **AWS CLI**
(aws-cli/2.35, path-style) against a live `buckit serve --port 0`, plus the
filesystem assertions — 32/32 checks passed.

- [x] `buckit serve ./s3data` prints the banner and creates `./s3data` with
      `.gitignore`=`*`, `buckets/`, `.tmp/`, `.multipart/`, `meta.sqlite`.
- [x] `aws s3api create-bucket --bucket uploads` → 200; `s3data/buckets/uploads/`
      exists.
- [x] `aws s3api list-buckets` → lists `uploads`.
- [x] `aws s3api head-bucket --bucket uploads` → 200; missing bucket → 404
      `NoSuchBucket`.
- [x] `aws s3api put-object --bucket uploads --key report.pdf --body report.pdf`
      → 200; `cmp s3data/buckets/uploads/report.pdf report.pdf` clean; `ETag` =
      hex MD5 of the file.
- [x] `put-object --key photos/cat.jpg …` → bytes at
      `s3data/buckets/uploads/photos/cat.jpg` (nested dirs created).
- [x] `aws s3api get-object --bucket uploads --key report.pdf out.pdf` →
      `cmp out.pdf report.pdf` clean.
- [x] `get-object --range bytes=0-99 … out.part` → HTTP 206; `out.part` is
      exactly 100 bytes = first 100 bytes of source.
- [x] `aws s3api head-object --bucket uploads --key report.pdf` → correct
      `ContentLength`, `ETag`, `ContentType`, `LastModified`.
- [x] `put-object --metadata team=infra …` then `head-object` → `Metadata`
      shows `{"team":"infra"}`.
- [x] `put-object --key 'weird:name?.txt' …` round-trips; on-disk filename is
      percent-encoded (no raw `:`/`?`); `head-object` reports key
      `weird:name?.txt`.
- [x] `aws s3api delete-object --bucket uploads --key report.pdf` → 200; file
      gone; subsequent `get-object` → 404 `NoSuchKey`.
- [x] `delete-bucket` on non-empty `uploads` → `BucketNotEmpty`; after emptying,
      → 200 and `s3data/buckets/uploads/` removed.
- [x] AWS CLI with a wrong secret key → 403 `SignatureDoesNotMatch`.
