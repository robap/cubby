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

## `serve` flags

| Flag | Default | Description |
| --- | --- | --- |
| `<DIR>` | — | Data directory (positional, required). Created on first run. |
| `--bind <ADDR>` | `127.0.0.1` | Address to bind. Use `0.0.0.0` to expose. |
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
to the S3 API needs CORS (`--cors`), a later flag; a presigned URL still works
from a browser, only cross-origin `fetch()` is gated.

## Web UI (Phase 5)

cubby serves a built-in **S3 debugger** at **`/_/`** — no extra process, no
Node, no build step for you; it ships inside the binary. Open
`http://127.0.0.1:9000/_/` after `serve`.

- **Live request log** (the home screen) — a live-streaming table of the S3
  requests *your app* makes: resolved operation (`PutObject`,
  `CreateMultipartUpload`, `UploadPart`, …), bucket/key, status (colored by
  class), duration, and byte counts. Filter as you type, pause (with an "N new"
  badge), and click a row to expand its full fields (`op`, `auth`, `error_code`).
  A big `s3.upload()` visibly decomposes into `CreateMultipartUpload` +
  N×`UploadPart` + `CompleteMultipartUpload` in real time, and a `403` shows its
  error code so you can see *why* at a glance. The same stream is on stdout (one
  aligned line per request — suppress with **`--quiet`**) and at
  `GET /_/api/events` as SSE or `?format=ndjson` for `jq`/tests.
- **Bucket browser** — a file-explorer over your buckets: folder navigation
  (prefix + `/` delimiter) with breadcrumbs, drag-and-drop upload into the
  current prefix, per-row download and delete, a **"+ New bucket"** button, and a
  substring **key search** (scoped to a bucket or across all buckets).
- **Object detail** — metadata (size, content-type, ETag, last-modified,
  `x-amz-meta-*`), inline preview for images / text / JSON, and a
  **presigned-URL** generator (GET/PUT + expiry picker) that mints a
  credential-less link.

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

**Building from source.** The UI lives in `web/` as a [`zero`](https://github.com/robap/zero)
project and is compiled into the binary by `build.rs`, so `zero` must be on
`PATH` to build cubby: `cargo install zero --locked`, then `cargo build`. The
build fails loudly if `zero` is missing. (Prebuilt binaries need none of this.)

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
