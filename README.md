# cubby

> The SQLite of S3 — a single MIT-licensed binary that stores objects as plain
> files on disk and starts in milliseconds with zero config.

cubby is an S3-compatible object store built for local development. Objects are
real files in a browsable directory tree (`ls` works, `cat` works), and all
metadata lives in one SQLite database. Delete the data directory for a factory
reset; copy it to clone your environment.

## Quick start

```console
$ ./cubby serve ./s3data
  S3 API   → http://127.0.0.1:9000   (access key: local / secret: localsecret)
  Web UI   → http://127.0.0.1:9000/_/
  Data dir → /home/you/project/s3data
```

On first run `serve` creates the data directory with this layout:

```text
./s3data/
  .gitignore        # contains "*" — the dir is disposable, like target/
  meta.sqlite       # metadata (WAL mode)
  buckets/          # object bytes, as real files at derived paths
    my-bucket/
      photos/cat.jpg
  .tmp/             # in-flight uploads (same filesystem → atomic rename)
  .multipart/       # staged multipart parts: <upload_id>/<part_number>
```

Point any S3 client at it with **path-style** addressing:

```console
$ aws --endpoint-url http://127.0.0.1:9000 s3api create-bucket --bucket uploads
$ aws --endpoint-url http://127.0.0.1:9000 s3api put-object \
      --bucket uploads --key report.pdf --body report.pdf
$ cat s3data/buckets/uploads/report.pdf   # the bytes are just a file
```

Configure the AWS CLI with the default credentials (`aws configure`): access key
`local`, secret key `localsecret`, any region.

## Installation

### From crates.io

```console
$ cargo install cubby
$ cubby serve ./s3data
```

Building needs only a Rust toolchain — the web UI ships pre-built inside the
crate, so there is no Node, no bundler, and nothing else to install.

### Docker / Podman

The image is published for **amd64 and arm64** (Apple Silicon runs natively),
built on `distroless/static` — just the static binary, no shell, a few MB.

```console
$ docker run -p 9000:9000 -v "$PWD/s3data:/data" ghcr.io/robap/cubby serve /data
```

The container binds `0.0.0.0` by default (via `CUBBY_BIND`) so the published port
is reachable from the host; `127.0.0.1` would not be. Object bytes land in the
mounted `./s3data` on your host, as real files — `cat s3data/buckets/...` works
exactly as it does for a native run.

**Podman** uses the identical image and command:

```console
$ podman run -p 9000:9000 -v "$PWD/s3data:/data" ghcr.io/robap/cubby serve /data
```

Rootless Podman maps the container's root user to *your* host user, so objects
written to the mounted directory are already owned by you — no extra flags.
(Don't reach for `--userns=keep-id` here: the image runs as root, so keep-id
would instead assign the files to a subordinate UID.)

> **Presigned URLs across the container boundary.** A URL signed inside the
> container for one host (e.g. a Compose service name) fails when fetched from
> the host under `localhost`, and vice versa — SigV4 signs the host. See the
> [Docker gotcha](#presigned-urls-query-string-auth) below.

## `serve` flags

| Flag | Default | Description |
| --- | --- | --- |
| `<DIR>` | — | Data directory (positional, required). Created on first run. |
| `--bind <ADDR>` | `127.0.0.1` | Address to bind. Use `0.0.0.0` to expose (env: `CUBBY_BIND` — the container image sets it). |
| `--port <PORT>` | `9000` | Port. `0` binds an ephemeral port, printed machine-parseably. |
| `--access-key <KEY>` | `local` | Access key clients must present (env: `CUBBY_ACCESS_KEY`). |
| `--secret-key <KEY>` | `localsecret` | Secret key clients sign with (env: `CUBBY_SECRET_KEY`). |
| `--quiet` | off | Suppress the per-request live-log line on stdout (see [Web UI](#web-ui-phase-5)). |
| `--seed <FILE>` | — | Seed buckets and fixture objects from a YAML file before the port binds (see [Seeding fixtures](#seeding-fixtures---seed-phase-6)). |

## Supported operations (Phase 1)

- **Buckets:** CreateBucket (any region accepted and ignored), ListBuckets,
  HeadBucket, DeleteBucket (only when empty).
- **Objects:** PutObject, GetObject (including a single `Range`), HeadObject,
  DeleteObject.
- **Auth:** header SigV4, validated against the configured credentials.
- **ETags:** hex MD5 of the object body, computed while streaming the write.
- **Content-Type** and user metadata (`x-amz-meta-*`) are stored and returned.

## Listing (Phase 2)

**ListObjectsV2** (`GET /<bucket>?list-type=2`) and legacy **ListObjects v1**
(`GET /<bucket>`) are supported, served entirely from SQLite — the `objects`
table is the clustered index scanned in key order, so there is no `readdir`.

- **`prefix`** — restrict to keys beginning with it.
- **`delimiter`** — an arbitrary string (commonly `/`); keys sharing the run up
  to the next delimiter roll up into `CommonPrefixes` ("folders"), emitted once
  via an index skip-scan rather than by walking every member.
- **`max-keys`** — page size; default 1000, silently **capped at 1000**. `0`
  returns an empty page; a negative value is `400 InvalidArgument`. Keys and
  common prefixes count **together** toward the limit.
- **`continuation-token`** (v2) — an **opaque** cursor for the next page; a
  malformed token is `400 InvalidArgument`. **`marker`** (v1) is the plaintext
  equivalent, with `NextMarker` returned only when a `delimiter` is set (an S3
  quirk — otherwise resume from the last `Key`).
- **`start-after`** (v2) — begin strictly after the given key on the first page.
- **`encoding-type=url`** — percent-encode `Key`/`Prefix`/`Delimiter`/
  `StartAfter`/`CommonPrefixes` in the response so XML-unsafe bytes round-trip;
  stored keys are unchanged.
- **`fetch-owner`** (v2) — include a fixed dev `Owner` (id = display name = the
  access key); v1 always includes it. **`StorageClass`** is always `STANDARD`.

Ordering is lexicographic by raw UTF-8 bytes, matching SQLite's default `BINARY`
collation and S3's own order.

## Multipart (Phase 3)

boto3 and the AWS CLI auto-switch to multipart upload for files larger than 8MB,
so the full five-verb lifecycle is supported:

- **CreateMultipartUpload** (`POST /<bucket>/<key>?uploads`) — allocates an
  opaque `upload_id` and captures the `Content-Type` and user metadata for the
  eventual object. Requires an existing bucket (`404 NoSuchBucket` otherwise).
- **UploadPart** (`PUT /<bucket>/<key>?partNumber=N&uploadId=…`) — streams one
  part to `.multipart/<upload_id>/<N>` with the same incremental-MD5/fsync path
  as PutObject, and returns the part's ETag (quoted hex MD5). Part numbers run
  `1..=10000` (outside → `400 InvalidArgument`); re-uploading a number replaces
  it (last write wins).
- **ListParts** (`GET /<bucket>/<key>?uploadId=…`) — lists recorded parts
  ascending with `PartNumber`/`Size`/`ETag`; `max-parts` defaults to and caps at
  1000, `part-number-marker` resumes strictly after.
- **CompleteMultipartUpload** (`POST /<bucket>/<key>?uploadId=…`) — validates the
  client's part list, then **assembles one real file**: the selected parts are
  streamed in ascending order into `.tmp/`, fsync'd, atomically renamed into
  `buckets/<b>/<key>`, and the object row is written last. Afterwards the object
  is byte-for-byte an ordinary object — `cat`, `cmp`, Range GET, HEAD, and
  listing all work unchanged.
- **AbortMultipartUpload** (`DELETE /<bucket>/<key>?uploadId=…`) — drops the part
  rows and the `.multipart/<upload_id>/` tree; no object is created.

An unknown `upload_id` on UploadPart/Complete/Abort/ListParts is `404
NoSuchUpload`. Complete rejects an empty part list with `InvalidRequest`, a
non-ascending list with `InvalidPartOrder`, and a missing or ETag-mismatched
part with `InvalidPart`.

**Composite ETag.** A completed multipart object's ETag is `md5-of-md5s-N`: the
hex MD5 of the concatenated **raw** 16-byte part digests (recorded at UploadPart,
never re-read from the data), suffixed `-<part count>`. A single-part upload
still gets `-1`. This is what real S3 returns, so sync tools (`rclone`, `aws s3
sync`) compare ETags correctly instead of re-transferring.

The 5MiB-minimum-part-size rule is **deliberately not enforced** (dev-tool
ergonomics — tests can drive the whole lifecycle with tiny parts).

## Presigned URLs, CopyObject & DeleteObjects (Phase 4)

### Presigned URLs (query-string auth)

An SDK can sign a short-lived URL that carries the whole SigV4 signature in the
query string (`X-Amz-Algorithm`, `X-Amz-Credential`, `X-Amz-Signature`,
`X-Amz-Expires`, …), so a browser, a `curl`, or a service with **no
credentials** can `GET` or `PUT` a single object. cubby validates these exactly
like header-signed requests — nothing to enable.

```console
$ aws --endpoint-url http://127.0.0.1:9000 s3 presign s3://uploads/report.pdf
http://127.0.0.1:9000/uploads/report.pdf?X-Amz-Algorithm=AWS4-HMAC-SHA256&…&X-Amz-Signature=…

$ curl "$URL" -o report.pdf        # no credentials needed
```

boto3 (`generate_presigned_url("get_object" | "put_object", …)`) and aws-sdk-js
v3 (`@aws-sdk/s3-request-presigner` `getSignedUrl`) generate the same URLs.
Create the boto3 client with `Config(signature_version="s3v4")` — boto3 defaults
a presigned **PUT** to the legacy SigV2 query scheme, which cubby (like MinIO)
does not accept; SigV4 is what modern apps use. A presigned `PUT` uses
`UNSIGNED-PAYLOAD`; the body still streams through the
normal write path and gets a correct content-MD5 ETag. A URL fetched after its
`X-Amz-Expires` window lapses is `403 AccessDenied` ("Request has expired"); a
URL with a tampered path or query is `403 SignatureDoesNotMatch`.

> **Docker gotcha — the host is part of the signature.** SigV4 signs the `Host`
> header, so a URL signed for `localhost:9000` will fail with
> `SignatureDoesNotMatch` if it is replayed against a different host:port — the
> classic Docker-Compose case where one container signs for `localhost` but the
> URL is used against the `cubby` service name (or vice versa). This is correct
> S3 behavior, not a bug: sign the URL for the **same** host the client will use
> to fetch it.

### CopyObject

`PUT /<destbucket>/<destkey>` with `x-amz-copy-source: /<srcbucket>/<srckey>`
(`s3.copy`, `s3.copy_object`, `aws s3 cp s3://a s3://b`) copies an existing
object's bytes into a new key. The bytes are streamed through the same
`.tmp/`→fsync→rename→row path as PutObject, so the copy is one real browsable
file (`cmp` against the source is clean).

- **ETag is preserved verbatim** — a single-part source keeps its hex MD5, a
  multipart source keeps its composite `-N` ETag (no re-hash).
- **`x-amz-metadata-directive: COPY`** (default) carries the source's
  content-type and user metadata onto the copy; **`REPLACE`** takes them from
  the request instead (content-type defaulting to `application/octet-stream`).
- **source == dest** with `REPLACE` is the metadata-only update idiom: the row's
  content-type/metadata are rewritten and the bytes are left untouched. With the
  default (`COPY`) directive a self-copy is `400 InvalidRequest`, matching S3.
- Errors, all before any bytes move: missing source bucket → `NoSuchBucket`,
  missing source key → `NoSuchKey`, missing destination bucket → `NoSuchBucket`.
  A non-`<bucket>/<key>` copy-source (access-point/Outpost ARN) →
  `NotImplemented`.

### DeleteObjects (batch)

`POST /<bucket>?delete` with an XML `<Delete>` body deletes up to **1000** keys
in one request (`s3.delete_objects`, and the engine behind `aws s3 rm
--recursive` / `rclone delete`). Each key follows the Phase 1 row-first-then-
unlink path in a single transaction. Deletion is **idempotent** — a key with no
object still reports as deleted, matching S3. A batch over 1000 keys is
`400 InvalidRequest`; a batch against a missing bucket is `404 NoSuchBucket`
(the whole request fails). `Quiet: true` returns an empty result on full success
(only errors would appear); `versionId` on an entry is ignored.

### Addressing

Path-style only (`http://host:port/<bucket>/<key>`). Virtual-host style
(`bucket.host`) is a later addition.

### Not yet implemented

ListMultipartUploads and UploadPartCopy (`x-amz-copy-source` on an UploadPart)
are not implemented (`NotImplemented`). Copy-source conditional headers
(`x-amz-copy-source-if-*`) are parsed and ignored. Browser cross-origin access
to the S3 API needs a per-bucket CORS config — set through the real
`PutBucketCors` S3 API, [documented below](#cors-browser-cross-origin-access-v02);
a presigned URL still works from a browser, only cross-origin `fetch()` is gated
until the bucket allows the page's origin.

## Web UI (Phase 5)

cubby serves a built-in **S3 debugger** at **`/_/`** — no extra process, no
Node, no build step for you; it ships inside the binary. Open
`http://127.0.0.1:9000/_/` after `serve` — or just visit
`http://127.0.0.1:9000/` in a browser and cubby redirects you there (a bare
`GET /` that asks for HTML and carries no SigV4 is treated as a human's browser;
signed S3 clients at `/` still get ListBuckets). The control follows the OS
light/dark setting by default and has a **theme toggle** cycling dark → light →
system (your choice is remembered across sessions).

- **Live request log** (the home screen) — a live-streaming table of the S3
  requests *your app* makes: resolved operation (`PutObject`,
  `CreateMultipartUpload`, `UploadPart`, …), bucket/key, a human **time-ago**
  column, status (colored by class), duration, and byte counts. Filter as you
  type, pause (with an "N new" badge), **Clear** the log, and click a row to
  expand its full fields (`op`, `auth`, `error_code`, timestamp) — including a
  **View object** link that jumps straight to that object in the browser. A big
  `s3.upload()` visibly decomposes into `CreateMultipartUpload` + N×`UploadPart`
  + `CompleteMultipartUpload` in real time, and a `403` shows its error code so
  you can see *why* at a glance. The same stream is on stdout (one aligned line
  per request — suppress with **`--quiet`**) and at `GET /_/api/events` as SSE or
  `?format=ndjson` for `jq`/tests.
- **Bucket browser** — a file-explorer over your buckets: folder navigation
  (prefix + `/` delimiter) with breadcrumbs, drag-and-drop upload into the
  current prefix, per-row download and delete, a **"+ New bucket"** button, and a
  substring **key search** (scoped to a bucket or across all buckets), and a
  per-bucket **Notifications** panel to add/remove webhook destinations (see
  [Event notifications](#event-notifications-webhook-v02)). Every location is a
  **deep link**: the selected bucket, an open folder prefix, and an open object
  each have their own URL, so you can bookmark or share one and the browser
  Back/Forward buttons move between them.
- **Object detail** — metadata (size, content-type, ETag, last-modified,
  `x-amz-meta-*`), inline preview for images / text / JSON / XML (JSON and XML
  are pretty-printed; tall previews scroll), and a **presigned-URL** generator
  (GET/PUT + expiry picker) that mints a credential-less link.

The log mirrors **client** S3 traffic only; uploads/deletes/bucket-creates done
*through the UI* go straight to storage and deliberately do not appear in it, so
the log stays an honest picture of what your app did.

> **The UI shares the S3 API's trust boundary — it is unauthenticated.** Running
> with `--bind 0.0.0.0` exposes the web UI (and the `/_/api/*` seam) to the
> network along with the S3 API. Keep cubby on `127.0.0.1` unless you mean to
> share it.

The presigned URLs minted by the UI's Generate button carry the same
**host-in-signature** constraint as SDK-generated ones — see the
[Docker gotcha](#presigned-urls-query-string-auth) above.

**Building from source.** Building cubby needs only a Rust toolchain:
`cargo build`. The web UI is a **committed build artifact** (`web/dist/`,
embedded by `src/embed.rs`), so `zero` is never on the build path. You only need
[`zero`](https://github.com/robap/zero) to *modify* the UI: edit `web/src`, run
`zero build` (`cargo install zero --locked` first), and commit the regenerated
`web/dist/`. There is no CI gate on UI freshness — regenerate before committing.

## Event notifications (webhook, v0.2)

Event-driven S3 is one of the most common shapes a real app takes: an object
lands → a thumbnail is generated, an import job kicks off, a row is cleaned up.
cubby lets you **develop and debug that flow locally** — it POSTs a JSON event to
a URL your app exposes when an object is created or removed, shaped byte-for-byte
like the event AWS delivers, so the handler you write against cubby runs
unchanged in prod. (This is the thing LocalStack paywalls and MinIO dropped.)

It is **opt-in**: with no destinations configured, nothing fires and nothing
changes.

### Configure per bucket — live, no restart

Notification config is **mutable bucket state** in `meta.sqlite` (not a startup
file), managed from the **bucket browser's Notifications panel** or the matching
JSON seam. Changes take effect immediately.

```
GET    /_/api/buckets/{bucket}/notifications          # list destinations
POST   /_/api/buckets/{bucket}/notifications          # add one → 201 + id
DELETE /_/api/buckets/{bucket}/notifications/{id}      # remove one → 204
```

A destination (the POST body, and one row of the GET):

```json
{
  "url": "http://localhost:3000/s3-hook",
  "events": ["s3:ObjectCreated:*", "s3:ObjectRemoved:*"],
  "prefix": "photos/",
  "suffix": ".jpg",
  "format": "s3-notification",
  "timeout_ms": 5000
}
```

- **`url`** — an **`http://`** endpoint cubby POSTs to. `https://` is rejected at
  write time (out of scope for v0.2 — see caveats).
- **`events`** — any of `s3:ObjectCreated:Put`, `:Copy`,
  `:CompleteMultipartUpload`, `s3:ObjectRemoved:Delete`, or a wildcard family
  `s3:ObjectCreated:*` / `s3:ObjectRemoved:*`.
- **`prefix`** / **`suffix`** — optional key filters (absent = no constraint),
  exactly like S3's `FilterRule`.
- **`format`** — `s3-notification` (default) or `eventbridge` (see below).
- **`timeout_ms`** — per-destination delivery timeout (default **5000**); raise
  it for a slow, cold-starting target (e.g. SAM Local in Docker).

**Validation happens at write time** — a bad destination (unknown event, invalid
`format`, non-`http://` url, non-positive `timeout_ms`) returns `400` and
persists nothing. Config is durable SQLite state: it survives restart, travels
with the data directory, and is removed when you delete the bucket (`aws s3 rb`
cascades its destinations away). Because the data dir is `.gitignore`'d, UI-set
config does **not** travel with the repo — a clean checkout / CI dir starts with
no destinations.

### Two payload formats

AWS does not emit one universal shape, so the `format` is per-destination:

- **`s3-notification`** — the `{"Records":[{"s3":{…}}]}` envelope that
  SQS/SNS/Lambda-direct integrations receive. `eventName` drops the `s3:` prefix
  (`ObjectCreated:Put`), the object `key` is URL-encoded (spaces as `+`),
  creations carry `size`/`eTag`, and removals omit both.
- **`eventbridge`** — the `{"source":"aws.s3","detail-type":"Object Created",
  "detail":{…}}` envelope the S3 → EventBridge path delivers. Note the field is
  lowercase `etag` here (vs. `eTag` in the other shape) — an AWS inconsistency
  cubby reproduces so handlers port cleanly — and `detail.reason` names the
  specific API (`PutObject`, `CopyObject`, …).

A handler that decodes either shape against cubby runs unchanged against the real
service.

### Filtering, per-path routing, and fan-out

- **Per-bucket is the native unit** (as in AWS); **per-path** is expressed with
  `prefix`/`suffix` filters. "Send `photos/` events to URL A and `invoices/` to
  URL B" is two filtered destinations on the one bucket.
- **Overlapping filters fan out** — if two destinations both match an object,
  **both** receive the POST. This is a deliberate divergence from AWS (which
  rejects overlapping rules): firing one object event at two local receivers is a
  legitimate thing to test.

### Delivery semantics

- **Never blocks the write.** Firing happens on a background task *after* the
  object is committed; the client's PUT/DELETE returns immediately. A slow,
  unreachable, or `500`-returning receiver cannot stall or fail an upload.
- **Best-effort, exactly one attempt — no retry.** A single POST bounded by
  `timeout_ms`; a connect failure, timeout, or non-2xx is **logged and dropped**.
  This is a dev tool, not a delivery bus (no durable outbox).
- **Visible in the live log.** Each delivery emits a synthetic line naming the
  destination and outcome (`→ webhook http://localhost:3000/s3-hook 200`), on
  stdout and at `GET /_/api/events`.
- **Fires at the store layer, regardless of origin.** A mutation made *through
  the Web UI* fires notifications too — an intentional divergence from the live
  log, which excludes UI mutations. **Seed writes do not fire** (they're startup
  fixtures, before the port binds).

### Try it — the bundled receiver

`examples/webhook_sink.rs` is a runnable receiver that prints each POST and
confirms it parses as an S3 event:

```console
$ cargo run --example webhook_sink -- --port 3000
# then add a destination with url http://127.0.0.1:3000/s3-hook and upload an object:
── POST /s3-hook
{ "Records": [ { "eventName": "ObjectCreated:Put", "s3": { … } } ] }
parsed as S3 event ✓  object key: photos/cat.jpg
```

Two flags exercise the delivery semantics: `--delay <ms>` (sleep before replying,
to trip a destination's `timeout_ms` while the upload still returns promptly) and
`--status <code>` (reply non-2xx, to see log-and-drop with no retry).

**Integration recipes** — cubby always does the same thing (POST the event to a
`url`); what runs at that url stands in for your prod routing target:

- **A plain HTTP endpoint in your app** (the common case) — a route that
  deserializes the body and runs your logic. Any language/framework.
- **A Lambda via an in-app bridge** — add a dev-only endpoint that deserializes
  cubby's body into your SDK's S3-event type and calls the *same* handler method
  the Lambda uses. Runs the real handler in-process and debuggable, no Lambda
  tooling.
- **A Lambda via AWS SAM Local** — point the `url` at
  `http://127.0.0.1:3001/2015-03-31/functions/<Fn>/invocations`; cubby's POST
  body *is* the invoke payload. Higher fidelity; **raise `timeout_ms`** (e.g.
  `20000`) so a container cold start isn't logged as a spurious timeout.

### Caveats (dev-tool honesty)

- **`http://` only** for v0.2 — no TLS stack is linked, so the static-musl /
  distroless build is untouched. `https://` is a later fast-follow.
- **`sequencer` is a local monotonic hex counter**, not AWS's value (documented
  as such; it's present and correctly typed).
- **Placeholder fields.** Values cubby has no real principal for
  (`userIdentity`, `requestParameters`, `responseElements`, `ownerIdentity`, the
  account id `000000000000`) are stable dev placeholders — don't expect a real
  identity.
- **No signed/authenticated webhooks** (HMAC header, SNS signatures) in v0.2 —
  plain POST to a local dev endpoint.

## CORS (browser cross-origin access, v0.2)

The real browser→S3 flow is: your **backend** mints a presigned URL (SDK
`generate_presigned_url` / `getSignedUrl`), hands it to your **frontend**, and
the frontend `fetch()`es it cross-origin — a page on `http://localhost:3000`
uploading to (or downloading from) cubby's S3 API on `http://localhost:9000`.
For the browser to allow that, the **bucket** must have CORS configured.

cubby honors the **same per-bucket S3 CORS API** as AWS — so your existing
`put-bucket-cors` bootstrap or terraform/CDK works unchanged, and a bucket with
no CORS behaves exactly like a fresh S3 bucket (the browser blocks it). It is
**mutable bucket state in the data dir**, not a server flag: it changes at
runtime with no restart and travels when you copy the dir.

### Configure per bucket — the S3 API, live, no restart

```bash
aws --endpoint-url http://localhost:9000 s3api put-bucket-cors \
  --bucket uploads --cors-configuration '{
    "CORSRules": [{
      "AllowedOrigins": ["http://localhost:3000"],
      "AllowedMethods": ["GET","PUT","POST","HEAD"],
      "AllowedHeaders": ["*"],
      "ExposeHeaders": ["ETag"],
      "MaxAgeSeconds": 600
    }]
  }'

aws --endpoint-url http://localhost:9000 s3api get-bucket-cors  --bucket uploads
aws --endpoint-url http://localhost:9000 s3api delete-bucket-cors --bucket uploads
```

- `PutBucketCors` **replaces** the whole config (AWS semantics). `GetBucketCors`
  on a bucket with none returns **`NoSuchCORSConfiguration`** (404) — the exact
  error an SDK's "does this bucket have CORS?" probe expects. `DeleteBucketCors`
  is idempotent (deleting when none exists still succeeds). Deleting the bucket
  cascades its CORS away.
- **Rule matching mirrors AWS**: rules are evaluated in order, first match wins;
  `AllowedOrigins` supports a bare `*` and a single wildcard segment
  (`https://*.example.com`); `AllowedMethods` ∈ {`GET`,`PUT`,`POST`,`DELETE`,`HEAD`};
  `AllowedHeaders` supports `*` (matched case-insensitively against the
  preflight's requested headers). A bare-`*` origin answers `*`; a specific or
  wildcard origin echoes the request origin with `Vary: Origin`.
- A cross-origin **preflight** (`OPTIONS` with `Origin` +
  `Access-Control-Request-Method`) is answered at the routing layer **before**
  auth — a preflight carries no signature — and shows up in the live log as a
  `Preflight` event (origin + allowed/rejected), so "why is my browser upload
  failing?" is answered in cubby's own stream. Actual responses (success **and**
  error) gain `Access-Control-Allow-Origin` + `Access-Control-Expose-Headers` so
  the browser lets JS read them (including the `ETag` an upload needs).
- The **Web UI** shows a bucket's CORS config read-only (the **CORS** button in
  the bucket browser) — management stays the S3 API, on purpose; the UI just
  makes visible which origins a bucket allows.

### The host-in-signature gotcha (same class as the Docker note)

A presigned URL is signed **for a specific `host:port`** and must be fetched at
that same origin — a URL signed for `localhost:9000` fails against
`127.0.0.1:9000`, and vice versa (see the
[Docker gotcha](#presigned-urls-query-string-auth) above). Note too that
`http://localhost:3000` and `http://localhost:9000` are **different origins** by
design — that's exactly the cross-origin boundary CORS governs; put your page's
origin (`:3000`), not cubby's (`:9000`), in `AllowedOrigins`.

Not in scope for v0.2: `Access-Control-Allow-Credentials` / cookie-authenticated
CORS (presigned URLs authenticate via the query signature, not cookies), and
CORS for the `/_/` web-UI seam (it's same-origin). A process-level allow-all
flag is deliberately **not** offered — it would hide a missing-CORS misconfig
that then breaks in prod.

## Seeding fixtures (`--seed`, Phase 6)

`cubby serve <dir> --seed seed.yaml` declares buckets and fixture objects in a
file and has them exist the instant the server is up — so a `seed.yaml` checked
into a repo boots every developer (and every CI run) the same object store.
This is what makes cubby a **test fixture**, not just a server. Without the flag,
startup behaves exactly as before (no buckets, no objects).

```yaml
# seed.yaml
buckets:
  - name: uploads
    objects:
      - key: hello.txt
        content: "hi there\n"       # inline UTF-8 literal
        content_type: text/plain
        metadata:
          team: platform            # becomes x-amz-meta-team
      - key: photos/logo.png
        file: ./tests/fixtures/logo.png   # raw bytes from disk
        content_type: image/png
  - name: reports                    # a bucket with no seeded objects
```

- Each object declares **exactly one** of `content:` (inline UTF-8) or `file:`
  (raw bytes; a relative path resolves against the seed file's own directory).
  `content_type:` and `metadata:` are optional and match what `PutObject` sets.
- **Seeded objects are real files.** Each is written through the same
  temp→fsync→rename→SQLite path as a client `PUT`, so it lands at
  `buckets/<b>/<key>` with a correct content-MD5 ETag — `cat` and `cmp` clean,
  indistinguishable from an uploaded object. The committed
  [`seed.yaml`](seed.yaml) is a runnable example.
- **Idempotent and declarative.** Buckets are created if missing (an existing
  one is left as-is); a named object overwrites whatever is there
  (last-writer-wins), while keys *not* in the seed are untouched. Re-serving the
  same seed is a no-op; edit it and re-serve to update the keys it names.
- **Fails fast.** Seeding runs after the data dir is prepared but **before the
  port binds**. Malformed YAML, an unknown field, an object with neither/both of
  `content`/`file`, or an unreadable `file:` prints a naming error to stderr and
  exits non-zero **without binding** — a broken fixture never looks like a
  running server. A half-applied seed (some fixtures written before the bad one)
  is fine for a dev tool; the loud exit is what matters.

Out of scope (per CONCEPT): adopting pre-existing loose files in `buckets/`
(that's a future `reindex`), and seeding anything beyond buckets and finished
objects (no multipart state, versions, or per-bucket config).

## Conformance matrix (Phase 6, the v0.1 promise)

cubby's compatibility promise is **executable, not claimed**: a GitHub Actions
workflow ([`.github/workflows/conformance.yml`](.github/workflows/conformance.yml))
runs five real S3 clients — **boto3**, **aws-sdk-js v3**, **aws-sdk-go-v2**,
**rclone**, and the **AWS CLI** — against a live cubby, each doing the three
things an app actually does:

1. **round-trip incl. a >8MB upload** that forces the multipart path (bytes
   verified equal),
2. **list a nested layout with a prefix + `/` delimiter** (expected keys and
   `CommonPrefixes`), and
3. **use a presigned URL** (the client's own signer; `rclone link` for rclone)
   fetched with no ambient credentials.

Each client is its own matrix job, so a single SDK regression shows up as one red
check. When the workflow is green on `main`, that's the signal to tag **v0.1**.

Run any client locally against the same harness:

```console
$ ./tests/conformance/run.sh boto3     # or: awscli | js | go | rclone
```

Locally a missing toolchain is warn-and-skipped; in CI (`CONFORMANCE_STRICT=1`)
it fails the job instead, so a real regression can't hide behind a green skip.

## Storage model

- **SQLite is the source of truth** for what exists; the filesystem holds bytes.
  An orphan file with no row reads as "does not exist".
- **Writes are streaming and atomic:** the body streams to `.tmp/` while being
  MD5-hashed (never buffering the whole object), is fsync'd, atomically renamed
  into `buckets/…`, and only then is the SQLite row inserted. A crash between
  rename and insert leaves a harmless orphan file.
- **Deletes** remove the SQLite row first, then unlink the file.
- The on-disk path is **derived** from the canonical key (percent-encoding the
  Windows-illegal set `<>:"|?*`, trailing dots/spaces, and reserved device
  names) and is never decoded back into a key.

## License

MIT. Forever.
