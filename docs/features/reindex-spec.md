# reindex — spec

**Status:** done · **Roadmap:** v0.2 (post-MVP, ad-hoc — not a numbered phase) · **Slug:** reindex

## Why

cubby's whole premise is **the filesystem is the API too**: objects are real
files under `buckets/<b>/<key>` that you can `ls`, `cat`, `cp`, and `tar`. The
CONCEPT (principle #2) sells "copy the directory = clone your dev environment;
tar it = share fixtures." Both invite the inverse move nobody has closed yet:
**put files *into* the tree by hand and have cubby serve them.**

Today that doesn't work. SQLite is the source of truth for *what exists*; the
filesystem only holds bytes. A file dropped into `buckets/uploads/report.pdf`
with no `objects` row is invisible — `aws s3 ls` won't show it, `GET` returns
`NoSuchKey`. The `--seed` feature (Phase 6) deliberately does **not** adopt
loose files; it writes *new* fixtures through the PutObject path. Its own spec
names this gap and defers it: "`reindex` (scan the tree, backfill SQLite) —
explicitly a v0.2 candidate."

`cubby reindex <dir>` closes it: scan `buckets/`, and for every real file with
no `objects` row, backfill the row so the file becomes a first-class object.
That makes the "seed by copying files in" workflow real, and doubles as a
recovery tool when `meta.sqlite` is lost, stale, or was never present (rebuild
the entire index from the byte tree alone).

North stars served:
- **The filesystem is the API too** — this is the north star made
  bidirectional. Until now the filesystem was a *read-through* of SQLite;
  reindex lets the filesystem *drive* SQLite, which is what "just drop files
  in" requires.
- **Dev tool first** — "I have a folder of fixtures, make it an S3 bucket" is a
  dev-ergonomics move, not a production one. It's an **offline** maintenance
  command (no port bound), so it composes with `cp -r`, `git checkout`, and
  `tar -x` in a shell script.
- **Starts in milliseconds, zero config** — reindex is opt-in and one-shot; it
  changes nothing about `serve`'s startup path.

## In scope

- A new **`cubby reindex <dir>`** subcommand (sibling to `cubby serve`).
  Operates on an existing data directory, opens `meta.sqlite`, scans
  `buckets/`, mutates the index, prints a summary, and **exits** — it never
  binds a port.
- **Bucket adoption.** Every directory directly under `buckets/` is a bucket:
  create its `buckets` row if one is missing (idempotent, like
  `create_bucket_if_missing`). The directory name **is** the bucket name
  (bucket names are not percent-encoded on disk).
- **Object backfill.** For every regular file under `buckets/<b>/**` whose
  recovered key has **no** `objects` row, insert a row with:
  - `key` — recovered from the file's bucket-relative path (see Behavior /
    key recovery).
  - `size` — the file's byte length (`stat`).
  - `etag` — the hex content-MD5 of the file's bytes, computed by streaming the
    file (never buffering it whole), i.e. a **single-part** ETag identical to
    what a plain `PutObject` of those bytes would produce.
  - `last_modified` — the file's modification time (mtime), as Unix seconds.
  - `content_type` — guessed from the file extension via a small built-in map
    (e.g. `.txt`→`text/plain`, `.json`→`application/json`, `.png`→`image/png`);
    falls back to `application/octet-stream` when unknown, exactly as the
    PutObject/seed default.
  - `metadata` — `{}` (user `x-amz-meta-*` metadata is not stored on disk and
    cannot be recovered).
- **Non-destructive, additive by default.** A file that already has an
  `objects` row is **left untouched** — its row (with any real content-type,
  user metadata, or multipart `-N` ETag a client set) is preserved, and its
  bytes are not re-hashed. Re-running reindex is therefore cheap and
  idempotent: a second run over an unchanged tree indexes zero new objects.
- **A summary to stdout** on success: counts of buckets adopted, objects
  indexed, and objects skipped (already indexed). Exit code `0`.
- **README/docs note** describing the "drop files in, then `cubby reindex`,
  then `cubby serve`" workflow, so the feature is discoverable.

## Out of scope

- **Pruning orphan rows** (an `objects` row whose backing file is gone). This
  reconcile direction — "delete the index down to match the tree" — is
  destructive and is deferred; see Open questions. The default run only *adds*.
- **Refreshing changed files** (a file whose bytes changed out-of-band while
  its row still shows the old size/ETag). Skip-if-present means such a row is
  left stale. Re-hashing every already-indexed file is expensive and would
  clobber recoverable-only-once metadata; deferred, see Open questions.
- **Reconstructing multipart `-N` composite ETags.** A file that a client
  originally uploaded via multipart has an ETag of `md5-of-md5s-N`; reindex has
  only the assembled bytes and no part boundaries, so it computes a *single-part*
  MD5 instead. Adopted objects get single-part ETags. (This only bites objects
  that were multipart *and* whose row was lost — the pure "copy files in"
  workflow never involves multipart.)
- **Recovering `content_type` / user metadata beyond the extension guess.**
  Not on disk, not recoverable.
- **In-flight multipart state** (`.multipart/`) and any `.tmp/` staging files —
  reindex indexes finished objects under `buckets/` only; it does not adopt or
  sweep those trees.
- **Running against a live server.** reindex is an offline command; concurrent
  use against a `serve` process on the same dir is not a supported v0.2
  workflow (WAL makes it non-catastrophic, but it's untested and unpromised).
- **Bucket-name validation.** Directory names are adopted verbatim as bucket
  names; a name S3 clients would reject (uppercase, `_`, too long) is the
  user's problem, consistent with cubby not validating names on CreateBucket.
- **CORS configs, notification destinations, or any non-object bucket state** —
  reindex reconstructs buckets and objects, nothing else.

## Behavior

### Invocation
```
cubby reindex ./s3data
```
- `<dir>` must be an existing cubby data directory (has `buckets/` and can open
  `meta.sqlite`). A missing/again-bootstrappable dir: reindex runs `bootstrap()`
  first (harmless, idempotent) so a bare tree with only `buckets/<b>/<files>`
  and no `meta.sqlite` still works — this is the "rebuild from bytes" case.
- No `--bind`/`--port`/credentials flags — reindex does not serve.

### What gets scanned
- Only the subtree **`buckets/<name>/…`**. cubby's own siblings (`meta.sqlite`,
  `.gitignore`, `.tmp/`, `.multipart/`) live *outside* `buckets/` and are never
  visited.
- Each **directory directly under `buckets/`** → a bucket (name = directory
  name). Adopted if it has no row.
- Each **regular file at any depth under a bucket dir** → a candidate object.
  Its key is the bucket-relative path with components recovered to key segments
  (below).
- **Skipped, with a counted note:** symlinks (not followed — avoids escaping
  the tree), and any non-directory entry sitting *directly* under `buckets/`
  (a loose file there belongs to no bucket).

### Key recovery (the central decision)
`key_to_relpath` (the PUT-time mapping) is **encode-only and injective**: every
byte that could be lossy on a filesystem — the Windows-illegal set, controls,
trailing dots/spaces, `%` itself — is percent-encoded, and because `%` is
always encoded, the mapping has an exact inverse. reindex recovers a key by
**reversing it**: split the bucket-relative path into components, percent-decode
each component, and join with `/`.

- For a plain drop-in file with no `%XX` sequences (the common case —
  `report.pdf`, `photos/cat.jpg`), the inverse is the identity: the key **is**
  the path. "What I copied in is the key."
- For a file cubby itself wrote for a tricky key (`a:b` → `a%3Ab` on disk),
  the inverse faithfully restores `a:b`, so an object survives a lost
  `meta.sqlite` round-trip with its original key intact.
- This deliberately makes reindex the **one** place cubby decodes a filename
  back into a key — the CONCEPT's "never decode filenames back into keys" rule
  is about the *serving* path (listings come from SQLite, never `readdir`), and
  reindex is the sanctioned exception. **Flagged in Open questions** for
  explicit sign-off, since it reverses a stated rule.

### Recoverable vs. lost fields
| Field | Source | Fidelity |
|-------|--------|----------|
| `key` | reverse of `key_to_relpath` | exact |
| `size` | `stat` | exact |
| `etag` | streamed MD5 of bytes | exact for single-part; **differs** for objects originally multipart |
| `last_modified` | file mtime | approximate (real mtime, not the original PUT time) |
| `content_type` | extension guess → octet-stream | best-effort |
| `metadata` | — | lost (`{}`) |

### Idempotence & re-runs
- Skip-if-present means the second run over an unchanged tree adopts 0 buckets
  and indexes 0 objects — a clean no-op that proves convergence.
- Dropping one new file into an already-indexed bucket and re-running indexes
  exactly that one file, touching nothing else.

### Summary output (shape, exact text settled in /plan)
```
reindexed ./s3data
  buckets: 2 adopted, 1 already present
  objects: 37 indexed, 4 already present, 1 skipped (symlink)
```

## Acceptance criteria

Named observers: the **AWS CLI** driving a `cubby serve` started *after*
reindex, plus **filesystem `cat`/`cmp`/`stat`** assertions. reindex itself is a
CLI command whose only output is SQLite state, so every criterion is proven by
serving the reindexed dir and asking a real client what it sees. Each becomes a
plan checkbox.

- [ ] **Loose file becomes a listable, downloadable object.** In a data dir,
      create `buckets/uploads/report.pdf` by hand (real bytes), with **no**
      `objects` row for it. `cubby reindex <dir>` then `cubby serve <dir>` →
      `aws s3 ls s3://uploads/` lists `report.pdf`, and
      `aws s3 cp s3://uploads/report.pdf -` returns bytes `cmp`-equal to the
      file on disk.
- [ ] **ETag equals the content-MD5 of the bytes.** For that adopted
      `report.pdf`, `aws s3api head-object --bucket uploads --key report.pdf`
      reports an `ETag` equal to the hex MD5 of the file's bytes (single-part
      form), and a `ContentLength` equal to its byte size.
- [ ] **A hand-made bucket directory is adopted.** Create
      `buckets/newbucket/` containing one file, with no `buckets` row. After
      reindex + serve, `aws s3 ls` lists `newbucket`, and
      `aws s3 ls s3://newbucket/` shows its object.
- [ ] **Nested prefixes recover as keys.** A file at
      `buckets/uploads/photos/cat.jpg` → after reindex + serve,
      `aws s3 ls s3://uploads/ --recursive` shows key `photos/cat.jpg`, and
      `aws s3 ls s3://uploads/photos/` with the default `/` delimiter shows it
      under that prefix.
- [ ] **Full rebuild from bytes alone.** Take a populated data dir, `rm
      meta.sqlite`, run `cubby reindex <dir>`, then `cubby serve <dir>` →
      `aws s3 ls` and per-bucket listings reproduce every bucket and object
      whose bytes remain under `buckets/` (keys, sizes, and single-part ETags
      match a fresh `head-object`).
- [ ] **Special-character key survives the round-trip.** `aws s3api put-object
      --bucket uploads --key 'weird:name.txt'` (server running), confirm
      `cat buckets/uploads/weird%3Aname.txt` shows the bytes; stop the server,
      `rm meta.sqlite`, `cubby reindex`, restart → `aws s3api head-object
      --bucket uploads --key 'weird:name.txt'` returns `200` with the right
      size (proving the percent-decode inverse recovered the `:` key).
- [ ] **content_type guessed; metadata absent.** After reindexing a
      hand-created `notes.txt`, `aws s3api head-object` shows
      `ContentType: text/plain` (extension guess) and an empty `Metadata` map.
- [ ] **Idempotent, non-destructive re-run.** Run `cubby reindex <dir>` twice.
      The second run's summary reports **0 objects indexed** (all already
      present). An object that already had a row before the first run — put via
      `aws s3api put-object` with `--content-type application/x-custom` and
      `--metadata k=v` — still reports that exact content-type and metadata via
      `head-object` after both reindex runs (its row was preserved, not
      overwritten by a guess).
- [ ] **Internal trees are ignored.** A stray file in `.tmp/` and a directory
      in `.multipart/` present at reindex time produce **no** buckets or
      objects — `aws s3 ls` shows only what lives under `buckets/`.

## Open questions

All four resolved (2026-07-19) — the recommended default was taken in each case:

- **Sanction decoding filenames → keys?** **Resolved: yes.** reindex reverses
  `key_to_relpath` to recover keys (a proven exact inverse — the only way a
  special-char key survives a lost `meta.sqlite`), scoped to reindex; a one-line
  CONCEPT note records it as the sanctioned exception to "never decode filenames
  back into keys."
- **Orphan rows (row present, file gone): prune or leave?** **Resolved:
  additive-only** — reindex only *adds* rows; it never prunes. Pruning is
  deferred to a later explicit `--prune` opt-in, not the v0.2 default.
- **Changed-file refresh: skip or re-hash?** **Resolved: skip** already-indexed
  files in v0.2 (fast; preserves content-type/metadata/`-N` ETags reindex can't
  reproduce). A `--force`/refresh mode is a later add.
- **content_type guess table — how rich?** **Resolved: a small hand-rolled
  extension table** (dev-common types, no new dependency), `application/octet-
  stream` fallback.
