# buckit

> The SQLite of S3 — a single MIT-licensed binary that stores objects as plain
> files on disk and starts in milliseconds with zero config.

buckit is an S3-compatible object store built for local development. Objects are
real files in a browsable directory tree (`ls` works, `cat` works), and all
metadata lives in one SQLite database. Delete the data directory for a factory
reset; copy it to clone your environment.

## Quick start

```console
$ ./buckit serve ./s3data
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
| `--access-key <KEY>` | `local` | Access key clients must present (env: `BUCKIT_ACCESS_KEY`). |
| `--secret-key <KEY>` | `localsecret` | Secret key clients sign with (env: `BUCKIT_SECRET_KEY`). |

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
credentials** can `GET` or `PUT` a single object. buckit validates these exactly
like header-signed requests — nothing to enable.

```console
$ aws --endpoint-url http://127.0.0.1:9000 s3 presign s3://uploads/report.pdf
http://127.0.0.1:9000/uploads/report.pdf?X-Amz-Algorithm=AWS4-HMAC-SHA256&…&X-Amz-Signature=…

$ curl "$URL" -o report.pdf        # no credentials needed
```

boto3 (`generate_presigned_url("get_object" | "put_object", …)`) and aws-sdk-js
v3 (`@aws-sdk/s3-request-presigner` `getSignedUrl`) generate the same URLs.
Create the boto3 client with `Config(signature_version="s3v4")` — boto3 defaults
a presigned **PUT** to the legacy SigV2 query scheme, which buckit (like MinIO)
does not accept; SigV4 is what modern apps use. A presigned `PUT` uses
`UNSIGNED-PAYLOAD`; the body still streams through the
normal write path and gets a correct content-MD5 ETag. A URL fetched after its
`X-Amz-Expires` window lapses is `403 AccessDenied` ("Request has expired"); a
URL with a tampered path or query is `403 SignatureDoesNotMatch`.

> **Docker gotcha — the host is part of the signature.** SigV4 signs the `Host`
> header, so a URL signed for `localhost:9000` will fail with
> `SignatureDoesNotMatch` if it is replayed against a different host:port — the
> classic Docker-Compose case where one container signs for `localhost` but the
> URL is used against the `buckit` service name (or vice versa). This is correct
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

The web UI at `/_/` (Phase 5, currently a `501` placeholder).
ListMultipartUploads and UploadPartCopy (`x-amz-copy-source` on an UploadPart)
are not implemented (`NotImplemented`). Copy-source conditional headers
(`x-amz-copy-source-if-*`) are parsed and ignored. Browser cross-origin access
to the S3 API needs CORS (`--cors`), a later flag; a presigned URL still works
from a browser, only cross-origin `fetch()` is gated.

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
