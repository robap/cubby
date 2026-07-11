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
  .multipart/       # reserved for multipart (Phase 3)
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

### Addressing

Path-style only (`http://host:port/<bucket>/<key>`). Virtual-host style
(`bucket.host`) is a later addition.

### Not yet implemented

Multipart uploads (Phase 3), presigned URLs and CopyObject/DeleteObjects
(Phase 4), and the web UI at `/_/` (Phase 5, currently a `501` placeholder).

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
