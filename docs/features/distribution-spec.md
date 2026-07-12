# Distribution — spec

**Status:** implemented (multi-arch/arm64 buildx pending CI) · **Roadmap:** ad-hoc (realizes the v1.0 "distribution" theme) · **Slug:** distribution

## Why

cubby is meant to be *"the SQLite of S3"* — something anyone can grab and run.
Today they can't. The build has a hard dependency on **`zero`** (github.com/robap/zero),
the Rust-binary frontend framework the UI is written in: `build.rs` shells out to
the `zero` CLI to compile `web/` → `web/dist/`, and `rust-embed` embeds that output.
`web/dist/` is git-ignored and rebuilt on every `cargo build`. `zero` is not yet
publicly installable, so **every distribution channel is blocked**:

- `cargo install cubby` runs `build.rs` on the user's machine, which would shell out
  to a `zero` binary they don't have (and docs.rs builds offline in a sandbox that
  couldn't run it anyway) → install fails for everyone but the author.
- The same failure appears inside a Docker builder stage.

The fix — **commit `web/dist/` as a tracked build artifact and stop invoking `zero`
from the build** — is not a compromise. It *restores* a stated design principle:

> CONCEPT principle #5: *"The whole repo builds with a Rust toolchain alone."*

That principle is currently **false** (you need `zero` on PATH). Committing the built
UI makes it true and unblocks all channels at once. This spec covers that foundation
plus the two channels the author wants: **crates.io** and a **container image** that
runs under both Docker and Podman.

North stars served: **#5 MIT + Node-free / Rust-toolchain-alone** (directly restored),
and the CONCEPT promise that cubby is a single binary anyone can run.

This spec formally flips the standing open question in CONCEPT.md and ROADMAP.md
("Commit `dist/` UI assets? Leaning no.") to **yes**. Updating those two docs' open-question
bullets is part of the work.

## In scope

- **Committed UI artifact.** `web/dist/**` becomes tracked (including `web/dist/.zero/fonts/**`).
  `build.rs` is removed entirely — `rust-embed` embeds the committed folder directly.
  `cargo build` / `cargo install` need only a Rust toolchain; `zero` is never on the
  build path. Regenerating the UI (`zero build`) is the developer's pre-commit
  responsibility; there is **no CI staleness gate** (CI has no `zero`) and **no committed
  git hook** — staleness is handled by developer discipline.
- **crates.io publishing.** Cargo.toml gains an explicit `include` (ship `web/dist/**`,
  exclude `s3data/`, `tests/`, `web/src/`, `web/.zero/`, `target/`, dev-only files) and
  publishing metadata (`repository`, `homepage`, `readme`, `keywords`, `categories`).
  `cargo publish --dry-run` is clean; a clean-machine install yields a working binary.
  First publish is **manual** (`cargo publish` by the author); no release automation.
- **Container image, multi-arch.** Multi-stage Dockerfile: a builder stage compiles a
  **static musl** binary, final stage is **`gcr.io/distroless/static-debian12`**. Published
  as a single **multi-arch manifest** covering **amd64 (`x86_64-unknown-linux-musl`)** and
  **arm64 (`aarch64-unknown-linux-musl`)** so Apple-Silicon Macs and Graviton run natively
  from the same image name (`docker buildx`). The container defaults its bind to `0.0.0.0`
  (127.0.0.1 is unreachable from the host). Works under **Docker and Podman**, including
  **rootless Podman** with host-owned data. First push to ghcr.io is **manual**.
- **Data as a mounted volume.** The data dir is a documented bind mount so objects remain
  real files the user owns on the host (principle #2).
- **README distribution section** covering: `cargo install cubby`, `docker run` / `podman run`
  invocations, the rootless-Podman volume-ownership note, and the reaffirmed Docker
  presigned-URL host-in-signature gotcha.

## Out of scope

- **cargo-dist prebuilt GitHub-release binaries.** Already named in CONCEPT/ROADMAP and
  *unblocked* by this work, but a separate follow-up channel — not proven here.
- **Publishing automation / release workflow** (auto `cargo publish`, auto image push to
  ghcr.io on tag). This spec proves the artifacts *build and run correctly*; wiring the
  release pipeline is a follow-up. (See open questions — first publish may be manual.)
- **`brew` tap** ("eventually" per CONCEPT).
- Any change to cubby's runtime behavior, S3 surface, or the UI itself. This is packaging
  only.
- Bundling CA roots for outbound HTTPS. Not needed until v0.2 webhooks; distroless already
  ships the cert bundle, so no action now.

## Behavior

**Build (post-change).** A fresh `git clone` with **no `zero` on PATH** builds cleanly:
`cargo build` embeds the committed `web/dist/`, `./cubby serve` serves the S3 API and the
UI under `/_/`. In debug builds `rust-embed` still reads `web/dist/` from disk; release
builds embed it. Editing the UI is a two-step dev loop: change `web/src`, run `zero build`,
commit the regenerated `web/dist/`.

**gitignore gotcha.** `web/.gitignore` currently ignores both `dist/` and `.zero/`. The
`.zero/` pattern also matches `web/dist/.zero/` (the embedded fonts), so simply un-ignoring
`dist/` is not enough — the change must ensure the entire `web/dist/**` subtree, fonts
included, is tracked (e.g. negate the nested path). The CSS references `/.zero/fonts/…`, so
missing fonts would ship a broken UI.

**crates.io packaging.** `cargo package --list` must show `web/dist/**` present and the
heavy/dev-only trees (`s3data/`, `tests/`, `web/src/`, `web/.zero/`) absent, so the crate is
small and `cargo install` compiles with a Rust toolchain alone. No build script runs at
install time (build.rs is gone), so docs.rs and offline installs work.

**Container.** The musl builder needs a musl C toolchain because `rusqlite`'s `bundled`
feature compiles SQLite from C source; the resulting binary is fully static
(`ldd` → "not a dynamic executable"). The final distroless/static image contains just that
binary. Default command binds `0.0.0.0` so `-p 9000:9000` is reachable from the host.
Distroless ships CA certs and a `nonroot` user, so the image is already forward-compatible
with v0.2 webhooks and can run non-root.

**Multi-arch.** The image is a manifest list bundling amd64 and arm64 builds under one
name, so `docker pull` / `podman pull` auto-selects the host architecture with no user
action. Apple-Silicon Macs (the bulk of current Mac dev machines) thus run cubby natively
rather than under slow amd64 emulation. Both are cross-compiled to their respective static
musl targets and assembled with `docker buildx`.

**Rootless Podman.** Under rootless Podman, container UIDs map into the user's namespace.
cubby's image runs as **root**, and rootless Podman's default mapping sends container-root
to the *invoking host user* — so a plain `podman run … -v "$PWD/s3data:/data"` already
leaves the mounted data owned by you, no extra flags. This is critical because cubby's
model is *the data dir is real files you own*. (Note: `--userns=keep-id` is **wrong** for a
root-running image — it would push the files to a subordinate UID instead. keep-id is for
images whose process runs as a non-root user matching yours.) Under rootful Docker there is
no user-namespace remapping, so ownership is likewise straightforward.

**Presigned-URL host gotcha (reaffirmed).** SigV4 signs `Host`; a URL signed for
`localhost:9000` fails against a container service name and vice versa. Existing README
paragraph stands; the Docker section links to it.

## Acceptance criteria

Each names a client + op or a filesystem assertion. This list becomes the plan's checkboxes.

**Foundation — committed UI, `zero`-free build**
- [ ] `git ls-files web/dist` lists `index.html`, `assets/app.*.js`, `assets/app.*.css`,
      `manifest.json`, and `.zero/fonts/*.woff2` → the built UI is tracked, fonts included.
- [ ] On a machine with **`zero` not on PATH**, `cargo clean && cargo build --release`
      succeeds, and `curl -s http://127.0.0.1:9000/_/` (after `./cubby serve`) returns the
      SPA HTML referencing the embedded `app.*.js` bundle → build needs no `zero`.
- [ ] `test -f build.rs` fails (file removed) and `cargo build` still embeds the UI → no
      build script in the install path.
- [ ] Edit any file under `web/src`, run `zero build`, then `git status --porcelain web/dist`
      shows changes → `web/dist/` is the regenerated tracked artifact.

**crates.io**
- [ ] `cargo package --list` includes `web/dist/index.html` and `web/dist/assets/…`, and
      does **not** include any `s3data/`, `tests/`, `web/src/`, or `web/.zero/` path.
- [ ] `cargo publish --dry-run` exits 0.
- [ ] From the packaged crate on a clean machine (Rust toolchain only, no `zero`/Node),
      `cargo install --path <unpacked>` produces a `cubby` binary; `cubby serve ./s3data`
      then serves `/_/` (200) — simulating `cargo install cubby`.
- [ ] `aws --endpoint-url http://127.0.0.1:9000 s3 mb s3://b && … cp …` round-trips an
      object against that cargo-installed binary, and `cat s3data/buckets/b/<key>` shows the
      real bytes.

**Container — Docker**
- [ ] `docker build -t cubby .` succeeds; extracting the binary and running
      `file`/`ldd` on it shows a **statically linked** executable.
- [ ] `docker run -p 9000:9000 -v "$PWD/s3data:/data" cubby serve /data` is reachable from
      the host with **no extra bind flag**: `aws --endpoint-url http://127.0.0.1:9000 s3 mb
      s3://b` and a put/get round-trip succeed, and `cat ./s3data/buckets/b/<key>` on the
      host shows the real bytes.
- [ ] `curl -s http://127.0.0.1:9000/_/` against the container returns the UI.

**Container — Podman (incl. rootless)**
- [ ] `podman build -t cubby .` succeeds with the same Dockerfile (no Docker-specific syntax).
- [ ] Plain rootless `podman run -p 9000:9000 -v "$PWD/s3data:/data" cubby serve /data`
      round-trips an object via the AWS CLI from the host, and `stat -c '%U' ./s3data/buckets/<b>/<key>`
      shows the **invoking user**, not `root` → host-owned data under rootless Podman (no
      `--userns=keep-id`; the image runs as root, which rootless Podman maps to you).

**Container — multi-arch**
- [ ] `docker buildx build --platform linux/amd64,linux/arm64` produces a manifest list;
      `docker buildx imagetools inspect <img>` (or `docker manifest inspect`) lists both
      `linux/amd64` and `linux/arm64` entries.
- [ ] On an arm64 host (Apple Silicon, or `--platform linux/arm64`), `docker run` of the
      image round-trips an object via the AWS CLI **without emulation** (the pulled image's
      arch matches the host).

## Decisions (resolved during refine)

- **No committed git hook.** UI staleness is handled by developer discipline alone.
- **First publish is manual** for both crates.io and ghcr.io; release automation is a
  separate follow-up (out of scope).
- **Multi-arch (amd64 + arm64) gates the milestone** — Apple-Silicon Macs run natively, not
  under emulation.

## Open questions

- **Names to confirm (factual, non-blocking).** crates.io crate name `cubby` is unclaimed;
  ghcr.io namespace is `ghcr.io/robap/cubby`. Verify the crate name is free before the
  manual publish (rename fallback if taken).
