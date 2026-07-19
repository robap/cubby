# reindex — plan

**Status:** done · **Spec:** [reindex-spec.md](reindex-spec.md) · **Roadmap:** v0.2 (post-MVP, ad-hoc — not a numbered phase)

## Approach

Add a second CLI subcommand, `cubby reindex <dir>`, that scans `buckets/` and
backfills `meta.sqlite` so hand-dropped files become first-class objects — the
inverse of today's SQLite-drives-filesystem flow, and the move *the filesystem
is the API too* has always implied. It is a **synchronous, offline** batch
(`std::fs` walk + streamed MD5; **no tokio runtime, no port, no `Store`, no
`Notifier`**) that mirrors the Phase-1 row shape but reads pre-existing bytes
instead of writing new ones — the minimal machinery a dev-tool maintenance
command needs (*dev tool first*). It touches only `Db` + `DataDir`, so it can't
accidentally fire webhooks or serve traffic.

Two load-bearing pure functions carry the correctness: a faithful **inverse of
`key_to_relpath`** (percent-decode per path component — sanctioned here as the
one place cubby decodes a filename back into a key, because the mapping is an
exact injective inverse), and a small **extension→content-type** table. The
engine is additive and non-destructive: it only *inserts* rows for files with
no row, leaving already-indexed rows (and their real content-type / user
metadata / multipart `-N` ETags) untouched — so re-runs are cheap and
idempotent (*starts in milliseconds, zero config* stays true; nothing about
`serve` changes). No new dependencies: `md-5`, `hex`, `percent-encoding`,
`anyhow` are already in `Cargo.toml`; recursion and the MIME table are
hand-rolled to keep the dep set minimal (*MIT + Node-free* ethos).

## Files
- `src/cli.rs` — add `Command::Reindex(ReindexArgs { dir })`; `ReindexArgs`
  carries just the data-dir path (no bind/port/creds). Update the CLI tests.
- `src/main.rs` — match `Command::Reindex`: `bootstrap()` the dir, `Db::open`,
  call `reindex::run`, print the summary, exit 0 (no tokio runtime on this arm).
- `src/keypath.rs` — new `relpath_to_key(rel: &Path) -> String`, the exact
  inverse of `key_to_relpath` (percent-decode each component, join with `/`),
  plus round-trip tests.
- `src/reindex.rs` — **new module.** The scan/backfill engine:
  `run(dirs: &DataDir, db: &Db) -> anyhow::Result<ReindexReport>`, the
  `ReindexReport` counts, the hand-rolled `buckets/` walk, per-file row
  synthesis (size/mtime/streamed-MD5/content-type/`{}`), and the
  extension→content-type guess helper.
- `src/lib.rs` — `pub mod reindex;`.
- `tests/s3_api.rs` (or a new `tests/reindex.rs`) — inner-loop integration
  tests: build a tree by hand, `reindex::run`, then drive the in-process server
  (`TestServer::spawn` against the same dir) to assert the objects list, size,
  and ETag, and that a pre-existing row's metadata survives.
- `tests/acceptance/reindex.sh` — end-to-end acceptance driving the real binary
  + AWS CLI + filesystem (mirrors `seed.sh`'s harness).
- `README.md` — document the `reindex` subcommand and the drop-files-in → serve
  workflow.
- `CONCEPT.md` — one sentence noting reindex as the sanctioned exception to
  "never decode filenames back into keys" (open-question #1 sign-off).

## Risks & unknowns
- **Decoding filenames → keys** reverses a stated CONCEPT rule. **Signed off
  (2026-07-19): yes, scoped to reindex.** Mitigated: the mapping is proven
  injective (`%` always encoded), so the inverse is exact; round-trip tests over
  the tricky cases (`:`, `%`, reserved names, trailing dots, nested) are the
  proof. The final Docs box records the CONCEPT exception.
- **Multipart `-N` ETags can't be reconstructed** — an adopted file that was
  originally multipart gets a single-part MD5, so its ETag changes. Documented
  as an inherent loss in the spec; acceptance uses single-part files, and the
  full-rebuild box compares against a *fresh* `head-object` (which also reports
  single-part), so it stays internally consistent.
- **Orphan rows / changed files** are deliberately *not* handled (additive-only,
  skip-existing) per spec Out-of-scope — **confirmed (2026-07-19):** no pruning,
  no rehash in v0.2. Both are later opt-in flags (`--prune` / `--force`).
- **Symlinks & loose files** under `buckets/` must be skipped (no-follow via
  `file_type().is_symlink()`, and non-dir entries directly under `buckets/`
  belong to no bucket) so reindex can't escape the tree or invent junk objects.

## Steps
Each box ≈ one small commit moving an observable behavior. Check only when the
outcome is real, not when code is written.

- [x] **CLI skeleton runs.** Add `Command::Reindex(ReindexArgs)` to `cli.rs` and
      wire `main.rs` to bootstrap + open the Db + call a stub `reindex::run`
      returning an empty report + print a summary. Observable: `cubby reindex
      --help` shows the subcommand; `cubby reindex ./freshdir` exits 0 and
      prints a `0 objects indexed` summary. CLI parse tests updated and green.
- [x] **`relpath_to_key` is an exact inverse.** Add the percent-decode-per-
      component inverse to `keypath.rs`. Observable: `cargo test` — for a battery
      (`report.pdf`, `photos/cat.jpg`, `a:b`, `100%.txt`, `CON`, `name.`, `..`),
      `relpath_to_key(key_to_relpath(k)) == k`; and a plain path with no `%XX`
      maps to itself.
- [x] **Content-type guessed from extension.** Add the hand-rolled
      extension→MIME table (`txt/json/html/css/js/png/jpg/jpeg/gif/svg/pdf`, …)
      returning `Option<&str>`. Observable: unit tests — `guess("notes.txt") ==
      Some("text/plain")`, `guess("blob.bin") == None` (caller defaults to
      `application/octet-stream`).
- [x] **Backfill engine adopts buckets + indexes new files.** Implement
      `reindex::run`: walk `buckets/<b>/**`, `create_bucket` for each bucket dir
      missing a row, and for each regular file with **no** `objects` row insert a
      row (`size`=stat, `etag`=streamed hex MD5, `last_modified`=mtime,
      `content_type`=guess-or-octet-stream, `metadata="{}"`); skip symlinks and
      loose files directly under `buckets/`. Observable: inner-loop test — build
      `buckets/uploads/report.pdf` + `buckets/uploads/photos/cat.jpg` +
      `buckets/newbucket/x` by hand, `reindex::run`, then `TestServer::spawn`
      lists all three keys with sizes and single-part ETags equal to a fresh
      `head-object`, and `newbucket` appears in ListBuckets.
- [x] **Skip-existing is non-destructive; summary counts are real.** Populate a
      row via a normal PUT (real content-type + metadata), then `reindex::run`.
      Observable: inner-loop test — the pre-existing object's content-type and
      metadata are unchanged after reindex; the returned `ReindexReport` reports
      it under `objects already present` (not indexed); a second `run` over the
      unchanged tree reports `0` buckets adopted and `0` objects indexed. `main`
      prints these counts.
- [x] **Acceptance script proves the criteria end-to-end.** Add
      `tests/acceptance/reindex.sh` (harness cloned from `seed.sh`): loose-file
      adoption, bucket-dir adoption, nested-prefix keys, full rebuild after `rm
      meta.sqlite`, special-character key round-trip, content-type guess +
      empty metadata, idempotent non-destructive re-run, and internal trees
      (`.tmp/`, `.multipart/`) ignored. Observable: `./tests/acceptance/reindex.sh`
      exits 0 with all checks PASS.
- [x] **Docs.** Update `README.md` with the `cubby reindex <dir>` command and the
      "copy files into `buckets/`, then reindex, then serve" workflow (and the
      inherent losses: single-part ETag for adopted files, no user metadata).
      Add the one-sentence CONCEPT note sanctioning decode-on-reindex.

## Acceptance
Mirrors the spec. `/implement` isn't done until every box passes by driving the
named client (AWS CLI against a `cubby serve` started *after* reindex) or a
filesystem assertion.

- [x] **Loose file becomes a listable, downloadable object.** Hand-create
      `buckets/uploads/report.pdf` (no row) → `cubby reindex` + `cubby serve` →
      `aws s3 ls s3://uploads/` lists `report.pdf`, and `aws s3 cp
      s3://uploads/report.pdf -` returns bytes `cmp`-equal to the file.
- [x] **ETag equals the content-MD5.** `aws s3api head-object … --key
      report.pdf` reports `ETag` = hex MD5 of the file's bytes and
      `ContentLength` = its size.
- [x] **Hand-made bucket directory adopted.** `buckets/newbucket/` (one file, no
      row) → after reindex + serve, `aws s3 ls` lists `newbucket` and
      `aws s3 ls s3://newbucket/` shows its object.
- [x] **Nested prefixes recover as keys.** `buckets/uploads/photos/cat.jpg` →
      `aws s3 ls s3://uploads/ --recursive` shows `photos/cat.jpg`, and
      `aws s3 ls s3://uploads/photos/` shows it under the `/`-delimited prefix.
- [x] **Full rebuild from bytes alone.** Populated dir → `rm meta.sqlite` →
      `cubby reindex` → `cubby serve` → `aws s3 ls` and per-bucket listings
      reproduce every bucket/object under `buckets/` (keys, sizes, single-part
      ETags match a fresh `head-object`).
- [x] **Special-character key survives.** `aws s3api put-object --key
      'weird:name.txt'`; confirm `cat buckets/uploads/weird%3Aname.txt`; stop
      server; `rm meta.sqlite`; `cubby reindex`; restart → `aws s3api
      head-object --key 'weird:name.txt'` returns `200` with the right size.
- [x] **content_type guessed; metadata absent.** Reindex a hand-created
      `notes.txt` → `aws s3api head-object` shows `ContentType: text/plain` and
      an empty `Metadata` map.
- [x] **Idempotent, non-destructive re-run.** Run `cubby reindex` twice: the
      second summary reports `0 objects indexed`; an object PUT earlier with
      `--content-type application/x-custom --metadata k=v` still reports that
      exact content-type and metadata via `head-object` after both runs.
- [x] **Internal trees ignored.** A stray file in `.tmp/` and a dir in
      `.multipart/` present at reindex time yield no buckets/objects — `aws s3
      ls` shows only what lives under `buckets/`.
