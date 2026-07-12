# CONCEPT

> **cubby** — one binary, one port, one directory.
> An S3-compatible object store built for local development — not a small
> production server pretending to be a dev tool.

## Why this exists

MinIO went closed (community edition in maintenance mode Dec 2025, repo
archived early 2026, admin UI paywalled before that). LocalStack gates S3
persistence behind its paid tier. Everything else in the space is either
production infrastructure wearing a dev-tool costume (SeaweedFS, Garage,
RustFS) or a bare API with no UI (Versity, s3proxy, S3Mock).

Nobody has built the **SQLite of S3**: a single MIT-licensed binary that
stores objects as plain files on disk, ships a built-in web UI, and starts
in milliseconds with zero config.

## Design principles

1. **The filesystem is the API too.** Objects are real files in a browsable
   directory tree. `ls` works. `cat` works. Integration tests can assert
   `Path("s3data/buckets/uploads/report.pdf").exists()` — no client, no mocks.
2. **One binary, one port, one directory.** Delete the directory = factory
   reset. Copy it = clone your dev environment. Tar it = share fixtures.
3. **Dev tool first.** Startup time, log readability, and test ergonomics are
   features. Replication, IAM, and erasure coding are not.
4. **MIT licensed. Forever.** The entire reason this project exists.
5. **No Node anywhere.** UI built with zero (Rust-binary frontend framework,
   MIT). The whole repo builds with a Rust toolchain alone.

## The experience

```
$ ./cubby serve ./s3data
  S3 API   → http://localhost:9000   (access key: local / secret: localsecret)
  Web UI   → http://localhost:9000/_/
  Data dir → /home/you/project/s3data

PUT  uploads/photos/cat.jpg   200   12ms   2.4MB
GET  uploads/photos/cat.jpg   200    3ms   2.4MB
```

- Binds `127.0.0.1` by default; `--bind 0.0.0.0` to expose.
- Credentials overridable via flags/env; option to accept any credentials.
- Data dir is created with a self-`.gitignore` (like `cargo` does for `target/`).
- `--port 0` binds an ephemeral port (printed machine-parseably) so parallel
  test suites can spin up isolated instances against temp dirs.
- Request log on by default; `--quiet` for CI.
- `--seed seed.yaml` creates buckets and loads fixture files on startup.

## Architecture

### Single port, routed

- Requests with SigV4 auth (header or `X-Amz-Signature` query) → S3 handler.
- `/_/` prefix → web UI + its JSON API. Underscore-prefixed bucket names are
  illegal in S3, so there is zero namespace collision.
- Bare `GET /` with `Accept: text/html` and no auth → redirect to `/_/`.
  Everything else at `/` → S3 ListBuckets XML.

### Data directory layout

```
./s3data/
  .gitignore        # contains "*"
  meta.sqlite       # metadata, listing, multipart state
  buckets/
    my-bucket/
      photos/cat.jpg
  .tmp/             # in-flight uploads (same filesystem → atomic rename)
  .multipart/       # {upload_id}/{part_number}
```

### Storage model

- **Bytes on disk, everything else in SQLite.** SQLite is the source of
  truth for what exists; the filesystem is where bytes live.
- Canonical S3 key stored in SQLite; filesystem path is *derived* from it
  (percent-encode the small Windows-illegal set — `<>:"|?*`, trailing
  dots/spaces, reserved names). Never decode filenames back into keys.
- Streaming writes: temp file in `.tmp/` → hash incrementally (never buffer
  whole objects) → fsync → rename into place → SQLite insert. Crash between
  rename and insert leaves a harmless orphaned file (sweepable). Deletes:
  SQLite row first, then unlink.
- Listing served from SQLite, not `readdir`: correct lexicographic order,
  delimiter/CommonPrefixes via skip-scan, continuation token = last key seen.

### SQLite schema (v0 sketch)

WAL mode, single writer (or small pool + busy_timeout).

- `buckets(name PK, created_at)`
- `objects(bucket, key, size, etag, content_type, last_modified,
  metadata JSON, PK(bucket,key)) WITHOUT ROWID` — the table itself is the
  clustered index ListObjectsV2 scans.
- `multipart_uploads(upload_id PK, bucket, key, content_type, metadata,
  started_at)`
- `multipart_parts(upload_id, part_number, size, etag,
  PK(upload_id,part_number))` — part MD5s recorded so the composite ETag is
  computed without re-reading data.

### Rust stack

- **Protocol: build on the `s3s` crate** (Apache 2.0). It handles the S3
  wire protocol, SigV4 (header + presigned/query-string), and XML; we
  implement its trait against the filesystem+SQLite backend. RustFS is built
  on it, so it survives real SDK traffic. This cuts the project roughly in
  half and removes the two highest-risk items (SigV4 and XML quirks).
  Its bundled `s3s-fs` backend is a toy; ours is the production-quality
  version of that idea.
- tokio + hyper (what `s3s` sits on); rusqlite (WAL) behind spawn_blocking;
  `md-5` for streaming ETags; `rust-embed` for UI assets; `clap` for CLI.
- Static musl build → `FROM scratch` Docker image as a secondary
  distribution channel (native binary is the primary story).

## S3 API surface (MVP)

The subset AWS SDKs actually exercise during app development:

- **Buckets:** Create, Delete, List, Head. Accept and ignore any region.
- **Objects:** Put, Get (with `Range`), Head, Delete, DeleteObjects (batch),
  CopyObject — including source==dest metadata-only updates (real SDK idiom).
- **ListObjectsV2** with prefix/delimiter/continuation-token/max-keys, plus
  legacy ListObjects v1. The most quirk-laden endpoint; delimiter semantics
  are what make "folders" work in every client.
- **Multipart:** Create/UploadPart/Complete/Abort/ListParts. Not optional —
  boto3 auto-switches to multipart at 8MB.
- **SigV4** header auth and presigned URLs (query-string auth).
- **Correct ETags:** content MD5, `md5-of-md5s-N` for multipart. Sync tools
  compare these.

### Deliberately NOT in MVP

CORS (fast-follow `--cors` flag — v0.2, needed only for browser→S3 direct
access like presigned frontend uploads; note in docs), versioning, object
lock, lifecycle, SSE/KMS, bucket policies/IAM, replication, storage classes,
tagging. First post-MVP promotion: **event notifications via webhook**
("POST to my app when an object lands" — LocalStack gates this behind Pro).

## Web UI

Built with **zero** (github.com/robap/zero) — dogfooding, and it keeps the
repo Node-free. UI is compiled static assets embedded in the binary; the
framework choice is invisible to users, so the bet carries no adoption risk.

**Keep the seam thin:** the UI speaks a small boring JSON API under
`/_/api/` (~8 endpoints: list buckets, list objects, object meta, presign,
delete, upload, events, health). No SigV4 in the UI. Portable if the
framework ever changes.

### Screens

1. **Live request log — the home screen.** SSE stream (`/_/api/events`) of
   every S3 request. This is the feature that makes the tool an *S3
   debugger*, not just a stand-in.
2. **Bucket browser:** folder-style navigation, upload (drag-drop),
   download, delete, breadcrumbs.
3. **Object detail:** metadata, content-type, size, ETag, inline preview
   for images/text/JSON, **presigned-URL button with expiry picker**.

### Live log design

- **SSE, not WebSocket** — strictly one-way, EventSource auto-reconnect,
  `Last-Event-ID` replay, plain HTTP on the existing port.
- Event: `{id, ts, method, op, bucket, key, status, duration_ms, bytes_in,
  bytes_out, auth, error_code}`. `op` (resolved S3 operation) turns the log
  into a teaching tool — watch one `s3.upload()` become
  CreateMultipartUpload + N×UploadPart + Complete. `error_code`
  (NoSuchKey, SignatureDoesNotMatch) answers "why 403" at a glance.
- No headers/signatures in the default stream (people screenshot logs).
- Server: `tokio::sync::broadcast` + ~1,000-event ring buffer. Lagged slow
  consumers get "dropped N" instead of causing backpressure. No persistence;
  resets on restart (correct for a dev log). ~150 lines.
- Same event struct feeds three consumers: SSE, pretty stdout line,
  `?format=ndjson` for piping into `jq`/test harnesses.
- UI: pause with "N new" badge, filter-as-you-type, color by status class,
  auto-scroll that stops when the user scrolls up, click-to-expand.
- Flourish (post-MVP): click a multipart event → highlight all events
  sharing its `upload_id`.
- Known zero stress tests: batch DOM insertions per animation frame at high
  event rates; head-evict/tail-append list reconciliation.

## Definition of done (v0.1)

boto3, aws-sdk-js v3, aws-sdk-go-v2, rclone, and the AWS CLI can all, in CI:

- round-trip uploads including one >8MB (forces multipart)
- list with prefixes + delimiters correctly (rclone is brutal about this)
- use presigned URLs (query-string auth)

**The compatibility matrix is the acceptance test, not the feature list.**
Path-style addressing required; virtual-host `bucket.localhost` is a v1.1
trick (`*.localhost` resolves without DNS setup).

## Build order

1. `s3s` skeleton + SQLite schema + Put/Get/Head/Delete with streaming and
   atomic writes → AWS CLI works.
2. ListObjectsV2 with full delimiter semantics → rclone stress test.
3. Multipart + composite ETags → boto3 100MB round-trip.
4. Presigned URLs (mostly free via `s3s`; test query auth per SDK).
5. Web UI: live log first, then bucket browser, then presign button.
6. `--seed` + CI conformance matrix.

Steps 1–4 ≈ 2–3 weeks of evenings given `s3s` does the protocol.

## Known sharp edges (document, don't solve)

- Presigned URLs embed the host in the signature: URLs signed for
  `localhost:9000` fail against `http://tool:9000` inside Docker Compose
  and vice versa. Inherent to SigV4; one README paragraph preempts the
  most common Docker issue report.
- Browser access to the S3 API needs `--cors` (v0.2); the presign button
  will tempt people to `fetch()` the URL from a frontend.

## Distribution

GitHub releases with prebuilt binaries (cargo-dist), Docker image on
ghcr.io, `brew install` eventually. README leads with `./cubby serve`;
Docker is a footnote. Compose snippet in README from day one.

## Open questions

- ~~Name.~~ **Decided: cubby.** (Note: "zero" the framework is nearly
  unsearchable — worth addressing before this project gives it public
  exposure.)
- ~~Commit `dist/` UI assets?~~ **Decided: yes.** `web/dist/` is a committed
  build artifact so cubby builds with a Rust toolchain alone (principle #5) and
  ships via crates.io/Docker without `zero`. Regenerate with `zero build` before
  committing; no CI freshness gate (CI has no `zero`). See
  `docs/features/distribution-spec.md`.
- `reindex` command (scan tree, backfill SQLite) for the "seed by copying
  files into the dir" workflow — v0.2 candidate.
- Language-specific test helpers (`with local_s3() as s3:`) — S3
  integration testing may be a bigger market than dev environments.
