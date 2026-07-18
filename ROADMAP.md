# ROADMAP

High-level development guide. This is the *sequence and shape* of the work,
not a task tracker. Each milestone names a **goal**, the **acceptance test**
that proves it done, and the **risk it retires**. When in doubt about scope,
re-read [CONCEPT.md](CONCEPT.md) — the design principles are the tiebreaker.

## North stars

Every decision is measured against these. If a feature conflicts with one,
the feature loses.

1. **Starts in milliseconds, zero config.** Startup time is a feature.
2. **The filesystem is the API too.** Objects stay browsable real files.
3. **Compatibility is proven, not claimed.** Real SDKs in CI, not hand-rolled
   request tests.
4. **Dev tool first.** Test ergonomics and log readability beat production
   feature-completeness every time.
5. **MIT + Node-free.** Non-negotiable; the reason the project exists.

## How we work

Development runs through a three-stage skill pipeline. Each stage emits a
durable markdown artifact under `docs/features/` that the next stage consumes,
and each **stops for review** before the next begins.

| Stage | Command | Reads | Writes |
|-------|---------|-------|--------|
| **Refine** | `/refine <feature>` | CONCEPT, ROADMAP | `docs/features/<slug>-spec.md` — the *what/why*; acceptance criteria as "which client proves it" |
| **Plan** | `/plan <slug>` | the spec + current code | `docs/features/<slug>-plan.md` — the *how*; an ordered checklist |
| **Implement** | `/implement <slug>` | the plan | code; checks the plan's boxes off as behavior lands |

Conventions:

- One `<slug>` threads all three files. Phase features are numbered
  (`02-listing-delimiters-…`) so `docs/features/` sorts into build order.
- A plan checkbox ≈ one small commit moving an observable behavior; checked
  only when that behavior is *observably* true, never when code is merely
  written.
- Implement is **TDD** (red→green→refactor) with `cargo clippy`/`fmt` clean and
  meaningful `llvm-cov` coverage per box. **Acceptance** boxes are proven by
  driving the real SDK/CLI named in the spec (via `/verify`), not by unit tests.
- The plan is a **living** doc — divergence is written back into it, not
  improvised around.

## Release train

| Version | Theme | Ships when |
|---------|-------|-----------|
| **v0.1** | MVP: the compatibility matrix passes | Build order steps 1–6 green in CI |
| **v0.2** | Browser-facing: per-bucket CORS API, event webhooks, `reindex` | v0.1 stable + first real users |
| **v1.0** | Hardening, docs, distribution polish | v0.1 API frozen, no known correctness bugs |
| **v1.1** | Virtual-host addressing (`*.localhost`) | v1.0 shipped |

Versions are themes, not deadlines. A milestone ships when its acceptance
test is green, not on a date.

---

## Phase 1 — Core object I/O (build-order step 1)

**Goal:** `s3s` skeleton wired to a filesystem + SQLite backend, with
streaming atomic writes for the four verbs every client starts with.

- `s3s` trait implementation scaffolded; single port, routed (S3 vs `/_/`).
- SQLite schema v0 (buckets, objects) in WAL mode.
- Bucket Create/Delete/List/Head.
- Object Put/Get (with `Range`)/Head/Delete.
- Streaming write path: `.tmp/` → incremental hash → fsync → rename → insert.
  Deletes: row first, then unlink.
- Data dir bootstrap: create dir + self-`.gitignore`, print the serve banner.

**Acceptance test:** the AWS CLI can create a bucket, put/get/head/delete an
object, and round-trip its bytes. `cat s3data/buckets/<b>/<key>` shows the
real file.

**Retires risk:** the storage model (atomic writes, crash safety, SQLite-as-
source-of-truth) is the foundation everything else sits on. Prove it first.

## Phase 2 — Listing & delimiter semantics (step 2)

**Goal:** ListObjectsV2 (and legacy v1) served from SQLite with correct
lexicographic order and folder semantics.

- Prefix, delimiter, `max-keys`, continuation tokens (last-key-seen).
- CommonPrefixes via skip-scan over the clustered `objects` index.

**Acceptance test:** `rclone` — brutal about delimiter correctness — lists,
syncs, and traverses nested prefixes without error.

**Retires risk:** the most quirk-laden endpoint. If listing is right, the
"folders work in every client" promise holds.

## Phase 3 — Multipart & correct ETags (step 3)

**Goal:** full multipart lifecycle and composite ETags, because boto3 auto-
switches to multipart at 8MB — this is not optional.

- Create / UploadPart / Complete / Abort / ListParts.
- `.multipart/{upload_id}/{part_number}` staging.
- Composite `md5-of-md5s-N` ETag computed from recorded part MD5s (no
  re-reading data).

**Acceptance test:** boto3 round-trips a 100MB object (forces multipart) and
the ETag matches what a sync tool expects.

**Retires risk:** the second-hardest correctness area after SigV4 (which
`s3s` handles for us).

## Phase 4 — Presigned URLs (step 4)

**Goal:** query-string (presigned) auth working across SDKs. Mostly free via
`s3s`; the work is verification, not implementation.

- CopyObject, including source==dest metadata-only updates (real SDK idiom).
- DeleteObjects batch.

**Acceptance test:** presigned PUT and GET URLs generated by each SDK resolve
correctly against the running server.

**Retires risk:** confirms the auth surface SDKs actually use in the wild.

## Phase 5 — Web UI (step 5)

**Goal:** the UI that makes this an *S3 debugger*, not just a stand-in. Built
with `zero`, embedded static assets, thin JSON seam under `/_/api/`.

Order within the phase matters — **live log first**, it's the wedge:

1. **Live request log** (home screen): SSE stream, `broadcast` + ring buffer,
   resolved `op` field, `error_code`, pretty stdout line, `?format=ndjson`.
2. **Bucket browser:** folder navigation, drag-drop upload, download, delete.
3. **Object detail:** metadata, inline preview, presigned-URL button.

**Acceptance test:** a human can watch one `s3.upload()` decompose into
CreateMultipartUpload + N×UploadPart + Complete in the live log, then browse
and download the result — no S3 client involved.

**Retires risk:** proves the differentiator. Everything before this is table
stakes; this is why someone chooses the tool.

## Phase 6 — Seed & conformance matrix (step 6) → **v0.1**

**Goal:** reproducible fixtures and the CI matrix that *is* the product spec.

- `--seed seed.yaml`: create buckets + load fixtures on startup.
- CI conformance matrix across **boto3, aws-sdk-js v3, aws-sdk-go-v2, rclone,
  AWS CLI**.

**Acceptance test (this is the v0.1 definition of done):** all five clients,
in CI:
- round-trip uploads including one >8MB (forces multipart)
- list with prefixes + delimiters correctly
- use presigned URLs

**Ship v0.1** when this matrix is green.

---

## Post-MVP

### v0.2 — Browser-facing & workflow

First promotions once real users arrive.

- **Per-bucket CORS API** (`PutBucketCors`/`GetBucketCors`/`DeleteBucketCors`,
  SQLite-backed bucket state). Needed for browser→S3 direct access (presigned
  frontend uploads); the presign button will tempt people to `fetch()`
  immediately. Honoring the real S3 API means a developer's existing
  `put-bucket-cors` bootstrap works unchanged. See `docs/features/cors-spec.md`.
- **Event notifications via webhook.** "POST to my app when an object lands."
  Highest-value post-MVP feature — LocalStack gates this behind Pro. Same
  event struct that already feeds the live log.
- **`reindex` command.** Scan the tree, backfill SQLite — supports the "seed
  by copying files into the dir" workflow.

### v1.0 — Hardening & distribution

- API frozen; no known correctness bugs against the matrix.
- Distribution: cargo-dist prebuilt binaries, ghcr.io Docker image
  (`FROM scratch` musl), Compose snippet in README. `brew` eventually.
- Docs cover the known sharp edges (esp. the presigned-URL host-in-signature
  Docker gotcha — one README paragraph preempts the most common issue).

### v1.1 — Virtual-host addressing

- `bucket.localhost` style addressing (`*.localhost` resolves without DNS
  setup). Path-style remains required and default.

### Beyond / candidates (not committed)

- Language-specific test helpers (`with local_s3() as s3:`) — potentially a
  bigger market than dev environments.
- Anything from the deliberate non-goals list only if the matrix demands it:
  versioning, object lock, lifecycle, SSE/KMS, IAM, replication, tagging.

---

## Explicit non-goals

Carried from CONCEPT.md so scope creep has a place to bounce off. Not in any
milestone unless a compatibility failure forces it:

Replication · IAM · erasure coding · versioning · object lock · lifecycle ·
SSE/KMS · bucket policies · storage classes · tagging.

This is a **dev tool**, not a production server in a costume.

## Open questions blocking milestones

- ~~**Name**~~ — decided: **cubby**. (`zero`'s searchability is still worth
  addressing before this project gives the framework public exposure.)
- ~~**Commit `dist/` UI assets?**~~ **Decided: yes** (distribution work) —
  `web/dist/` is committed so cubby builds with a Rust toolchain alone and ships
  via crates.io/Docker without `zero`. See `docs/features/distribution-spec.md`.
- **Single-writer SQLite under one instance hit concurrently** — confirm the
  WAL + `busy_timeout` story holds (parallel *test suites* sidestep this via
  isolated dirs, but a single shared instance does not).
