# Seed & conformance matrix — spec

**Status:** implemented — `--seed` done & verified; conformance matrix authored and
locally green for all five clients, awaiting the first GitHub Actions run to
observe green-on-`main` (→ tag v0.1) · **Roadmap:** Phase 6 (→ v0.1) · **Slug:**
06-seed-conformance-matrix

## Why

This is the last MVP phase and the **v0.1 definition of done**. It ships two
things, both about *reproducibility*:

1. **`--seed seed.yaml`** — declare buckets and fixture objects in a file and
   have them exist the instant the server is up. This is the ergonomic that
   makes cubby a *test fixture*, not just a server: check a `seed.yaml` into a
   repo and every developer (and every CI run) boots the same object store.
   It's also how the conformance matrix and future demos get a known starting
   state without a client round-trip.

2. **The CI conformance matrix** — the compatibility promise made *executable*.
   Per CONCEPT, "the compatibility matrix **is** the acceptance test, not the
   feature list." Five real clients (boto3, aws-sdk-js v3, aws-sdk-go-v2,
   rclone, AWS CLI) each do the three things an app actually does — round-trip
   incl. a >8MB multipart upload, list with prefixes + delimiters, use a
   presigned URL — against a live cubby in GitHub Actions. When it's green,
   v0.1 ships.

North stars served:
- **Compatibility is proven, not claimed** — five independent SDK/CLI
  implementations exercising the real wire protocol in CI is *the* deliverable;
  this phase is that north star made literal.
- **The filesystem is the API too** — seeded objects are **real files** at
  `buckets/<b>/<key>` (`cat`/`cmp` clean), written through the same Phase 1
  temp→fsync→rename→row path; a seed file plus the data dir it produces is a
  tar-able, diff-able fixture.
- **Dev tool first / starts in milliseconds, zero config** — seeding is a
  startup step measured in fixtures, not migrations; a malformed seed fails
  fast *before* binding the port so a broken fixture never looks like a running
  server.

## In scope

### `--seed <path>`
- New `--seed <FILE>` flag on `cubby serve` (a `PathBuf`, no default). Absent =
  today's behavior exactly (no buckets, no objects created).
- **YAML** seed format (CONCEPT/ROADMAP both name `seed.yaml`). Declares:
  - **Buckets** to create.
  - **Objects** per bucket, each with a key and content from **either**:
    - `content:` — an inline literal string (UTF-8), or
    - `file:` — a path to a local file whose raw bytes are loaded (relative
      paths resolve against the seed file's own directory).
  - Optional per-object `content_type:` and `metadata:` (a string→string map),
    matching what PutObject would set.
- **Load path:** every seeded object is written through the **Phase 1 write
  path** (stage into `.tmp/` → fsync → atomic rename into
  `buckets/<b>/<key>` → SQLite row last), so it is a real browsable file with a
  correct content-MD5 ETag and is indistinguishable from a client `PUT`.
- **Timing:** seed runs after `bootstrap()` and Db open, **before the port
  binds**. The banner prints only once seeding succeeds.
- **Re-run semantics (idempotent, declarative):**
  - Buckets: created if missing; an already-present bucket is left as-is (no
    error).
  - Objects: a seeded key **overwrites** whatever is there (last-writer-wins,
    same as PutObject) so the running state always matches the seed file for
    the keys it names. Keys *not* mentioned in the seed are left untouched.
- **Fail fast:** malformed YAML, an unknown field, an object with neither/both
  of `content`/`file`, or a `file:` that doesn't exist / can't be read → cubby
  prints a clear error to stderr and exits **non-zero without binding the
  port**. A half-applied seed (some fixtures written before the bad one) is
  acceptable for a dev tool; the exit is what matters.
- A **committed example `seed.yaml`** (in `docs/` or repo root) so the flag is
  self-documenting and the README can point at it.

### CI conformance matrix
- A **GitHub Actions workflow** (the repo has **no CI yet** — this phase
  bootstraps `.github/workflows/`) that, on push/PR, builds cubby once and runs
  the five-client matrix.
- **Per client** (boto3, aws-sdk-js v3, aws-sdk-go-v2, rclone, AWS CLI), against
  a live server on an isolated temp data dir (`--port 0` + the machine-parseable
  port line, one instance per client job):
  1. **Round-trip incl. multipart** — upload and download an object, with at
     least one **>8MB** upload that forces the multipart path, bytes verified
     equal.
  2. **List with prefixes + delimiters** — create a nested key layout and list
     it with a prefix + `/` delimiter, asserting the expected keys and
     CommonPrefixes (rclone's traversal is the brutal case).
  3. **Presigned URL** — generate a presigned GET (via the client's own signer;
     `rclone link` for rclone) and fetch it with no ambient credentials,
     getting the object bytes.
- **Two new client harnesses** this phase adds (Phases 2–4 already drive boto3,
  aws-sdk-js v3, AWS CLI in `tests/acceptance/`): **aws-sdk-go-v2** (a small Go
  program) and **rclone** (a remote config pointing at the path-style HTTP
  endpoint).
- The workflow installs each client's toolchain (python/node/go/rclone/awscli)
  and reports **red/green per client** so a single SDK regression is obvious.

## Out of scope

- **`reindex`** (scan the tree, backfill SQLite) — explicitly a **v0.2**
  candidate in CONCEPT/ROADMAP. Seeding writes *new* fixtures through the write
  path; it does not adopt pre-existing loose files in `buckets/`.
- **Seed of anything beyond buckets + objects** — no multipart-upload state, no
  presigned-URL entries, no versions, no per-bucket config. A fixture is
  buckets and finished objects.
- **A seed "generator"** (e.g. "make me a random 100MB object") — the matrix's
  >8MB requirement is satisfied by the *test* generating its own large upload at
  runtime, not by seed. Large fixtures come via `file:`.
- **Deleting/reconciling** keys absent from the seed (no "sync to match the
  file exactly"). Seed only creates/overwrites what it names.
- **New S3 endpoints or protocol behavior.** The matrix *exercises* Phases 1–4;
  any gap it exposes is a bug fixed against the existing spec, not new surface
  here.
- **CORS / browser presigned uploads** (`--cors`) — v0.2; the matrix's
  presigned checks are server-to-server / curl, no cross-origin JS.
- **Distribution artifacts** (cargo-dist binaries, ghcr.io image, brew) — v1.0.
  This workflow proves conformance; it does not publish releases.
- **Non-linux CI runners** — matrix is `ubuntu-latest` for v0.1; multi-OS is a
  later hardening concern.

## Behavior

### Seed file shape (illustrative)
```yaml
buckets:
  - name: uploads
    objects:
      - key: hello.txt
        content: "hi there\n"
        content_type: text/plain
        metadata:
          team: platform
      - key: photos/logo.png
        file: ./fixtures/logo.png        # bytes loaded from disk
  - name: reports          # bucket with no seeded objects
```
- Exact key naming/nesting of the schema is settled in `/plan`; the **shape**
  above (top-level `buckets`, each with `name` and optional `objects`; each
  object exactly one of `content`/`file`, plus optional `content_type`/
  `metadata`) is the contract the acceptance criteria assume.
- `content` is UTF-8 text; binary fixtures use `file:`.
- ETag of a seeded object = content-MD5 of its bytes (single-part), identical to
  a client `PUT` of the same bytes.
- Ordering: buckets created in file order, objects within a bucket in file
  order; a later duplicate key wins (consistent with overwrite semantics).

### Startup flow with `--seed`
1. `bootstrap()` the data dir (unchanged).
2. Open the Db (unchanged).
3. **Apply the seed** (new): parse file → for each bucket, create-if-missing →
   for each object, write through the Phase 1 path (overwrite). On any error,
   print `error: …` to stderr and exit non-zero **before** the listener binds.
4. Print the banner and bind the port (unchanged).

### Conformance matrix behavior
- Each client job is self-contained: start a fresh cubby on a temp dir, run the
  three checks, tear it down. Isolation is by data dir + ephemeral port, the way
  `tests/acceptance/*.sh` already do it.
- **Presigned per client:** boto3 `generate_presigned_url`, aws-sdk-js v3
  `@aws-sdk/s3-request-presigner`, aws-sdk-go-v2 `s3.PresignClient`, AWS CLI
  `aws s3 presign`, rclone `rclone link`. Each produces a URL fetched with a
  credential-less HTTP client returning the object bytes and `200`.
- **rclone remote:** configured for path-style, plain-HTTP `endpoint`,
  `region us-east-1`, static creds — the config a user would write for a local
  S3. rclone `sync`/`lsf` drive the round-trip and listing checks.
- A client whose toolchain can't be provisioned (e.g. transient network)
  **fails the job loudly** — the matrix does not silently skip a client (that
  would let a real regression hide behind a "skipped"). Local dev scripts may
  still warn-and-skip; CI must not.

## Acceptance criteria

Named observers: the **AWS CLI** + filesystem `cat`/`cmp`/`Path` for the seed
feature (deterministic, no SDK needed), and the **five matrix clients**
(boto3, aws-sdk-js v3, aws-sdk-go-v2, rclone, AWS CLI) driven in CI for the
conformance matrix. Each box becomes a plan checkbox.

### `--seed`
- [ ] **Buckets appear.** `cubby serve <dir> --seed seed.yaml` where `seed.yaml`
      lists buckets `uploads` and `reports` → `aws s3 ls` shows both, and
      `Path("<dir>/buckets/uploads")` and `Path("<dir>/buckets/reports")` exist.
- [ ] **Inline fixture is a real file.** A seed object `hello.txt` with
      `content: "hi there\n"` → `cat <dir>/buckets/uploads/hello.txt` prints
      `hi there`; `aws s3 cp s3://uploads/hello.txt -` returns those exact bytes;
      the reported `ETag` equals the MD5 of `hi there\n`.
- [ ] **File-backed fixture loads real bytes.** A seed object
      `photos/logo.png` with `file: ./fixtures/logo.png` →
      `cmp <dir>/buckets/uploads/photos/logo.png fixtures/logo.png` is clean and
      `aws s3api get-object … /dev/stdout` returns the same bytes.
- [ ] **content_type + metadata are applied.** A seed object declaring
      `content_type: text/plain` and `metadata: {team: platform}` →
      `aws s3api head-object` shows `ContentType: text/plain` and
      `x-amz-meta-team: platform` (i.e. `Metadata.team == "platform"`).
- [ ] **Re-run is idempotent and declarative.** Serving the same seed against an
      existing dir succeeds (no "bucket exists" error); editing an inline
      `content` and re-serving → `aws s3 cp s3://…/hello.txt -` now returns the
      **new** bytes; a key created out-of-band and *not* in the seed is still
      present afterward (untouched).
- [ ] **Malformed seed fails fast, no bind.** A seed with invalid YAML — or an
      object referencing a nonexistent `file:` — makes `cubby serve … --seed …`
      exit **non-zero**, print an error naming the problem, and leave **nothing
      listening** on the port (a follow-up `aws s3 ls` / `curl` connection is
      refused).
- [ ] **No `--seed`, no change.** `cubby serve <dir>` without the flag creates no
      buckets — `aws s3 ls` on a fresh dir is empty (baseline that seeding is
      opt-in).

### Conformance matrix (the v0.1 definition of done)
Each client, running in GitHub Actions against a live cubby, passes all three
checks. (Plan may split each into three boxes; grouped here per client.)
- [ ] **boto3** — round-trips an object incl. a **>8MB** upload (auto-multipart)
      with bytes verified equal; lists a nested prefix with `/` delimiter
      getting the expected keys + CommonPrefixes; fetches a
      `generate_presigned_url` GET (no creds) returning the bytes.
- [ ] **aws-sdk-js v3** — same three: multipart round-trip (`Upload` from
      `@aws-sdk/lib-storage` on a >8MB body), delimiter list, and a
      `getSignedUrl` GET fetched credential-less.
- [ ] **aws-sdk-go-v2** — same three: `manager.Uploader` >8MB round-trip,
      `ListObjectsV2` with prefix+delimiter, and `s3.PresignClient` GET.
- [ ] **rclone** — `rclone copy`/`sync` round-trips a >8MB file (multipart via
      rclone's chunker), `rclone lsf`/traversal over nested prefixes is correct,
      and `rclone link` yields a presigned URL that `curl` fetches to the bytes.
- [ ] **AWS CLI** — `aws s3 cp` round-trips a >8MB object (CLI auto-multipart),
      `aws s3 ls s3://b/prefix/` shows the delimiter'd listing, and
      `aws s3 presign` yields a URL `curl` fetches to the bytes.
- [ ] **The matrix runs in CI and gates.** A `.github/workflows/` job builds
      cubby and runs all five client jobs on push/PR; the workflow is
      **green** on `main` and its red/green status is visible per client. This
      green run is the signal to tag **v0.1**.

## Open questions

- **YAML parser crate.** `serde_yaml` is archived (unmaintained since early
  2024). Candidates: `serde_norway` (maintained `serde_yaml` fork), `serde_yml`,
  or `saphyr`. Adds the repo's first YAML dep. Which one? *(Implementation
  detail — resolved in `/plan`; doesn't change the acceptance criteria, but
  flagging the dependency choice now.)* **Recommendation:** `serde_norway` for
  the drop-in serde ergonomics.
- **CI matrix structure.** GitHub Actions `strategy.matrix` with **one job per
  client** (parallel, clean isolation, obvious per-client red/green) vs. a single
  job looping a shared runner script. **Recommendation:** job-per-client matrix
  over a shared bash conformance runner each client job invokes with its name.
  Confirm before `/plan`.
- **Should this workflow also run the Rust inner loop** (`cargo test` +
  `clippy`/`fmt`)? The repo has no CI at all, so bootstrapping the full gate now
  is tempting. **Recommendation:** add a separate `build-test` job in the same
  workflow (build/test/clippy/fmt) alongside the conformance jobs — adjacent
  scope, but it's the natural moment. Confirm whether to include it or keep this
  phase strictly the five-client matrix.
- **Where do committed fixtures live?** Proposed: an example `seed.yaml` at repo
  root (README points at it) + matrix fixtures under `tests/fixtures/`. Confirm
  location/naming in `/plan`.
