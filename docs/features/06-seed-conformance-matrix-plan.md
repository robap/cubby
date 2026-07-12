# Seed & conformance matrix — plan

**Spec:** [06-seed-conformance-matrix-spec.md](06-seed-conformance-matrix-spec.md) · **Roadmap:** Phase 6 (→ v0.1)

## Approach

Two deliverables, independent enough to build in either order but planned
seed-first (the matrix can *use* a seeded fixture).

**`--seed`** is a new `src/seed.rs` module plus one CLI flag. It parses a YAML
file into typed serde structs, then applies it **before the port binds** by
reusing the Phase 1 write path — so a seeded object is a real
`buckets/<b>/<key>` file with a correct content-MD5 ETag, indistinguishable from
a client `PUT` (*the filesystem is the API too*). To keep a single write path
(correctness), we factor `Store::put_object`'s post-stream logic
(temp→fsync→rename→row-last) into a reusable `Store::put_bytes` helper that both
`put_object` and the seed loader call — no second, drifting implementation.
Seeding runs inside `serve()` before `TcpListener::bind`; any error returns
`anyhow::Err`, so a broken fixture exits non-zero and **never looks like a
running server** (*starts in milliseconds, zero config* — a fixture either
boots clean or fails loud).

**The conformance matrix** is a GitHub Actions workflow (the repo has **no CI
yet** — this bootstraps `.github/workflows/`) driving five real clients through
three checks each — >8MB multipart round-trip, prefix+delimiter listing,
presigned URL — against a live cubby on an ephemeral port. It extends the
existing `tests/acceptance/*.sh` convention: a shared runner starts cubby with
`--port 0`, parses the machine-parseable banner line for the address (the
pattern `listing.sh`/`presigned_copy_batch.sh` already use), and dispatches to a
per-client harness. boto3 / aws-sdk-js v3 / AWS CLI harnesses have Phase 2–4
precedent; **aws-sdk-go-v2** and **rclone** are new. This *is* the v0.1
acceptance test (*compatibility is proven, not claimed*).

## Files

- `src/cli.rs` — add `--seed <FILE>` (`Option<PathBuf>`) to `ServeArgs`; extend
  the parse tests.
- `src/http.rs` — add `seed: Option<PathBuf>` to `ServeConfig`; in `serve()`,
  apply the seed (build a `Store`, call `seed::apply`) **before**
  `TcpListener::bind`.
- `src/main.rs` — pass `args.seed` into `ServeConfig`.
- `src/store.rs` — factor a reusable `put_bytes`/`put_reader` write helper out of
  `put_object` (temp→fsync→rename→row, MD5 computed); `put_object` delegates to
  it. Add a `create_bucket_if_missing` convenience if not already covered by
  `db.create_bucket` returning a bool.
- `src/seed.rs` *(new)* — `SeedFile`/`SeedBucket`/`SeedObject` serde structs
  (`deny_unknown_fields`, exactly-one-of `content`/`file`); `async fn apply(path,
  &Store) -> anyhow::Result<()>`.
- `src/lib.rs` — `pub mod seed;`.
- `Cargo.toml` — add `serde_norway` (maintained `serde_yaml` fork) for YAML.
- `seed.yaml` *(new, repo root)* — committed example the README points at.
- `tests/conformance/run.sh` *(new)* — shared runner: start cubby `--port 0`,
  parse addr, dispatch to `<client>` harness, tear down.
- `tests/conformance/{boto3,awscli,js,go,rclone}/…` *(new)* — the five client
  harnesses (Go program for go-v2; node script for js; rclone remote config).
- `tests/fixtures/` *(new)* — small committed seed fixtures (e.g. `logo.png`).
  The matrix's >8MB object is **generated at runtime** by the runner
  (`head -c ~12000000 /dev/urandom` into the temp dir, matching
  `multipart.sh`/`presigned_copy_batch.sh`) — never committed to git.
- `tests/s3_api.rs` — inner-loop integration tests for `seed::apply` (parse,
  overwrite, fail-fast) using the in-process `TestServer` harness where possible.
- `.github/workflows/conformance.yml` *(new)* — build cubby once; matrix job per
  client; (optional) `build-test` job.
- `README.md` — document `--seed` + the example, and add a conformance/CI note.

## Risks & unknowns

- **YAML crate.** `serde_yaml` is archived; using **`serde_norway`** (maintained
  fork) — drop-in serde ergonomics, `deny_unknown_fields` support. First YAML
  dep in the repo; keep it confined to the seed module.
- **Reusing the write path from `serve()`.** `put_object`'s logic is an `async`
  method on `Store`. Factoring `put_bytes` must not change `put_object`'s
  observable behavior (ETag, content-type default, metadata JSON) — the existing
  Phase 1 tests are the guard. Seeding needs MD5 computed (unlike
  `stage_file_copy`, which preserves ETags), so the helper streams through the
  `stream_to_temp` discipline, reading from bytes (inline) or a file (`file:`).
- **Fail-fast ordering.** Seed must apply *after* `bootstrap()`+`Db::open` but
  *before* `bind`. A partially-applied seed on error is acceptable (spec);
  what's load-bearing is the non-zero exit with nothing listening.
- **rclone presigned.** `rclone link` on the S3 backend produces a presigned GET
  — confirm it targets the object (not a public-ACL link) against a path-style
  plain-HTTP remote. If `rclone link` misbehaves, fall back to asserting
  rclone's own signed round-trip + a `curl` of an `aws presign` URL, but prefer
  the client's own signer for a true five-way presign proof.
- **CI toolchain install cost.** Five toolchains (python/node/go/rclone/awscli).
  Matrix job-per-client keeps installs parallel and failures per-client legible.
  rclone + awscli come from apt/official installers; pin versions for
  reproducibility.
- **No "skip" in CI.** Local scripts warn-and-skip a missing toolchain; the CI
  matrix must **fail** instead, or a real regression hides behind a green skip.
- **`--seed` + `--port 0` isolation.** Each matrix client uses a fresh temp dir;
  the seed (if used) is applied per instance — no cross-job state.

## Steps

Each box ≈ one small commit moving an observable behavior. Check only when the
outcome is real, not when code is written.

- [x] **`--seed` flag parses** — add `--seed <FILE>` (`Option<PathBuf>`) to
      `ServeArgs`, thread `seed: Option<PathBuf>` through `ServeConfig` and
      `main.rs`. Observable: `cubby serve --help` lists `--seed`; the `cli.rs`
      parse test asserts the flag maps to the path (absent → `None`).
- [x] **Seed structs parse YAML** — add the YAML crate; define
      `SeedFile`/`SeedBucket`/`SeedObject` in `src/seed.rs` with
      `deny_unknown_fields` and exactly-one-of `content`/`file`. Observable: a
      unit test parses the committed `seed.yaml` into the structs, and rejects
      (a) an unknown field and (b) an object with both/neither `content`+`file`.
- [x] **Factor `Store::put_bytes`** — extract `put_object`'s
      temp→fsync→rename→row-last logic into a reusable helper that computes the
      content-MD5, defaults content-type, and serializes metadata; `put_object`
      delegates. Observable: existing Phase 1 `put_object` integration tests
      stay green (pure refactor, no behavior change).
- [x] **Seed creates buckets + inline objects** — `seed::apply(path, &Store)`
      creates each bucket (create-if-missing, no error if present) and writes
      each `content:` object via `put_bytes`; call it from `serve()` before
      `bind`. Observable: `cubby serve dir --seed seed.yaml` then `aws s3 ls`
      shows the buckets and `cat dir/buckets/<b>/hello.txt` shows the inline
      bytes.
- [x] **Seed loads `file:` objects** — resolve `file:` relative to the seed
      file's directory, stream the file's bytes through `put_bytes`. Observable:
      `cmp dir/buckets/<b>/<key> fixtures/<f>` is clean for a binary fixture.
- [x] **Seed applies content_type + metadata** — carry `content_type:` and the
      `metadata:` map onto the written row. Observable: `aws s3api head-object`
      shows the declared `ContentType` and `x-amz-meta-*`.
- [x] **Bad seed fails fast** — malformed YAML, an unknown field, or a missing
      `file:` makes `serve()` return `Err` before binding. Observable: the
      process exits non-zero, prints an error naming the problem, and nothing
      listens on the port (a follow-up `curl`/`aws s3 ls` is connection-refused).
      Covered by an integration test asserting the exit + no-bind.
- [x] **Conformance runner + boto3 & AWS CLI** — `tests/conformance/run.sh
      <client>` starts cubby `--port 0`, parses the banner addr, generates the
      shared >8MB test object (`head -c ~12000000 /dev/urandom` into the temp
      dir), and dispatches; wire the boto3 and AWS CLI harnesses (3 checks each:
      >8MB multipart round-trip, prefix+delimiter list, presigned GET).
      Observable: `run.sh boto3` and `run.sh awscli` pass locally, exit 0.
- [x] **aws-sdk-js v3 harness** — node script: `@aws-sdk/lib-storage` `Upload`
      (>8MB), `ListObjectsV2` with delimiter, `getSignedUrl` GET fetched
      credential-less. Observable: `run.sh js` passes locally.
- [x] **aws-sdk-go-v2 harness** — small Go program: `manager.Uploader` (>8MB),
      `ListObjectsV2` prefix+delimiter, `s3.PresignClient` GET. Observable:
      `run.sh go` passes locally.
- [x] **rclone harness** — path-style plain-HTTP remote config: `rclone copy`
      (>8MB, multipart), `rclone lsf` over nested prefixes, `rclone link` →
      `curl` fetches the bytes. Observable: `run.sh rclone` passes locally.
- [ ] **CI workflow** — `.github/workflows/conformance.yml`: build cubby once,
      `strategy.matrix` job per client installing its toolchain and running
      `run.sh <client>`; runs on push/PR. Observable: the workflow is **green**
      on `main` with per-client status visible. Scope is the five-client matrix
      only — the Rust inner loop (`cargo test`/`clippy`/`fmt`) is deliberately
      **not** added here; it lands later with the full CI, so it isn't
      duplicated now.
- [x] **Docs** — update `README.md`: the `--seed` flag with the example
      `seed.yaml`, and a short conformance/CI section (what the matrix proves =
      the v0.1 promise). Add/point at the committed `seed.yaml`.

## Acceptance

Mirrors the spec. `/implement` isn't done until every box passes by driving the
named client. Seed boxes use the **AWS CLI** + filesystem `cat`/`cmp`/`Path`;
matrix boxes drive the five clients in CI.

### `--seed`
- [x] **Buckets appear.** `cubby serve <dir> --seed seed.yaml` (lists `uploads`,
      `reports`) → `aws s3 ls` shows both; `Path("<dir>/buckets/uploads")` and
      `.../reports` exist.
- [x] **Inline fixture is a real file.** `hello.txt` with `content: "hi there\n"`
      → `cat <dir>/buckets/uploads/hello.txt` prints `hi there`; `aws s3 cp
      s3://uploads/hello.txt -` returns those bytes; `ETag` = MD5 of `hi
      there\n`.
- [x] **File-backed fixture loads real bytes.** `photos/logo.png` with `file:
      ./fixtures/logo.png` → `cmp <dir>/buckets/uploads/photos/logo.png
      fixtures/logo.png` clean; `aws s3api get-object` returns the same bytes.
- [x] **content_type + metadata applied.** Object with `content_type:
      text/plain`, `metadata: {team: platform}` → `aws s3api head-object` shows
      `ContentType: text/plain` and `Metadata.team == "platform"`.
- [x] **Re-run is idempotent + declarative.** Re-serving the same seed succeeds
      (no "bucket exists" error); editing an inline `content` and re-serving →
      `aws s3 cp s3://…/hello.txt -` returns the **new** bytes; an out-of-band
      key not in the seed is still present afterward.
- [x] **Malformed seed fails fast, no bind.** Invalid YAML or a missing `file:`
      → `cubby serve … --seed …` exits non-zero, prints a naming error, and
      leaves nothing listening (connection refused).
- [x] **No `--seed`, no change.** `cubby serve <dir>` without the flag → `aws s3
      ls` on a fresh dir is empty.

### Conformance matrix (v0.1 definition of done — in CI)
- [x] **boto3** — >8MB auto-multipart round-trip (bytes equal); nested prefix +
      `/` delimiter list (expected keys + CommonPrefixes); `generate_presigned_url`
      GET fetched no-creds returns the bytes.
- [x] **aws-sdk-js v3** — `lib-storage` `Upload` >8MB round-trip; delimiter list;
      `getSignedUrl` GET fetched credential-less.
- [x] **aws-sdk-go-v2** — `manager.Uploader` >8MB round-trip; `ListObjectsV2`
      prefix+delimiter; `s3.PresignClient` GET.
- [x] **rclone** — `rclone copy`/`sync` >8MB round-trip; `rclone lsf` nested
      traversal correct; `rclone link` URL `curl`-fetched to the bytes.
- [x] **AWS CLI** — `aws s3 cp` >8MB round-trip; `aws s3 ls s3://b/prefix/`
      delimiter listing; `aws s3 presign` URL `curl`-fetched to the bytes.
- [ ] **The matrix runs in CI and gates.** `.github/workflows/` builds cubby and
      runs all five client jobs on push/PR; green on `main`, per-client status
      visible. This green run is the signal to tag **v0.1**. *(Workflow authored
      and locally validated — see Progress notes; awaiting the first GitHub
      Actions run to observe green-on-main, which cannot be checked locally.)*

## Progress notes

- **Committed acceptance script.** Added `tests/acceptance/seed.sh` (not in the
  original Files list) mirroring the `tests/acceptance/*.sh` convention. It drives
  the real `cubby` binary + AWS CLI + filesystem `cat`/`cmp` and asserts all
  seven `--seed` acceptance criteria (17 checks, all green). This is the outer
  loop for the seed feature; `seed_*` in `tests/s3_api.rs` is the inner loop.
- **Per-client harness shape.** Each conformance client is a `check.sh` entry
  point (uniform dispatch from `run.sh`) plus its check program:
  `boto3/check.py`, `js/check.mjs` (+ `package.json`), `go/main.go`
  (+ `go.mod`/`go.sum`, committed for reproducibility), and bash for
  `awscli`/`rclone`. `run.sh <client>` generates the shared >8MB object once and
  exports it. Local runs warn-and-skip a missing toolchain; `CONFORMANCE_STRICT=1`
  (set by CI) fails instead.
- **`Store::put_bytes` signature.** The factored helper takes a pre-serialized
  `metadata_json: String` (both `put_object` and the seed loader serialize their
  own map with one identical `serde_json::to_string` call) rather than
  serializing a map internally — the load-bearing single write path (temp→fsync→
  rename→row-last) is what's shared; metadata-to-JSON is a trivial serde call, not
  a second implementation. `create_bucket_if_missing` was added on `Store` (dir +
  idempotent row), since `db.create_bucket` alone doesn't make the directory.
- **Streaming, not buffering.** `seed::apply` builds a `StreamingBlob` per object
  (a one-shot stream for inline `content`, a `ReaderStream` over the opened file
  for `file:`) so even a large `file:` fixture streams through `put_bytes` without
  being held in memory. A `file:` is *opened in `apply`* (not lazily in the
  stream) so a missing/unreadable file fails fast with a message naming the path.
- **YAML crate.** `serde_norway` 0.9 (maintained `serde_yaml` fork), confined to
  `src/seed.rs`, with `deny_unknown_fields` on every struct.
- **CI observability caveat.** The workflow (`.github/workflows/conformance.yml`)
  is authored and validated as far as is possible without GitHub: the YAML parses,
  and the exact command each matrix job runs — `CONFORMANCE_STRICT=1
  tests/conformance/run.sh <client>` — passes locally for **all five** clients
  (rc=0). The only unobservable-locally part is a green run on GitHub's own
  runners; the "workflow green on `main`" Steps box and the "matrix runs in CI"
  Acceptance box stay unchecked until that first push. Scope is the five-client
  matrix only; the Rust inner loop (test/clippy/fmt) is intentionally not added
  here (lands with full CI later).
