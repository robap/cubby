# Distribution — plan

**Spec:** [distribution-spec.md](distribution-spec.md) · **Roadmap:** ad-hoc (realizes the v1.0 "distribution" theme)

## Approach

Three channels rest on one foundation. **First**, make the build `zero`-free by
committing `web/dist/` as a tracked artifact and deleting `build.rs` — `rust-embed`
already embeds `web/dist/` (`src/embed.rs`), so once the folder is tracked nothing needs
to *produce* it at build time. This restores CONCEPT principle #5 (*"builds with a Rust
toolchain alone"*), which the `zero`-on-PATH requirement currently breaks. **Then** the two
publish channels fall out cheaply: crates.io needs only package metadata + an explicit
`include` (and `cargo package`'s own verification build proves the `zero`-free build from
packaged sources); the container is a multi-stage Dockerfile that cross-compiles a static
musl binary into `gcr.io/distroless/static`, published multi-arch via `buildx`.

Two non-obvious choices, each pinned to reality in the code:

- **Container bind via `env`, not `CMD`.** The spec's acceptance runs
  `docker run … cubby serve /data`, which *overrides* any default `CMD` args — so baking
  `--bind 0.0.0.0` into `CMD` would be silently lost and the container would bind
  `127.0.0.1` (unreachable). Instead add `env = "CUBBY_BIND"` to the existing `--bind` flag
  (`src/cli.rs:38`) and set `ENV CUBBY_BIND=0.0.0.0` in the image. The env survives command
  override, the CLI default stays `127.0.0.1` (no runtime-behavior change for non-container
  use), and it mirrors the existing `CUBBY_ACCESS_KEY`/`CUBBY_SECRET_KEY` idiom.
- **`.gitignore` anchoring.** `web/.gitignore`'s `.zero/` pattern also matches
  `web/dist/.zero/` (the embedded fonts the CSS references). Anchor it to `/​.zero/` so only
  the top-level framework cache stays ignored and `web/dist/**` — fonts included — is
  tracked. Serves north star #2 (the UI ships whole) and #5.

## Files

- `web/.gitignore` — anchor `/​.zero/`; drop `dist/` so `web/dist/**` is tracked.
- `web/dist/**` — newly committed built UI (index.html, `assets/app.*`, `manifest.json`,
  `.zero/fonts/*.woff2`).
- `build.rs` — **deleted**; UI is a committed artifact, not built at compile time.
- `src/embed.rs` — module docs + the `index_html_is_embedded…` test comment updated to say
  the committed `web/dist/` is embedded (no `build.rs` reference). No logic change.
- `src/cli.rs` — add `env = "CUBBY_BIND"` to the `--bind` arg; default unchanged.
- `Cargo.toml` — add `repository`, `homepage`, `readme`, `keywords`, `categories`, and an
  explicit `include` (`src/**`, `web/dist/**`, `Cargo.toml`, `README.md`, `LICENSE`).
- `LICENSE` — new MIT license file (crates.io/docs.rs surface; CONCEPT principle #4).
- `Dockerfile` — new; multi-stage static-musl builder → distroless/static, `ENTRYPOINT
  ["/cubby"]`, `ENV CUBBY_BIND=0.0.0.0`.
- `.dockerignore` — new; keep build context small (`target/`, `s3data/`, `.git/`, `tests/`).
- `README.md` — new Distribution/Install section; correct the "Building from source" note.
- `CONCEPT.md`, `ROADMAP.md` — flip the "Commit `dist/`? Leaning no" open-question bullets
  to the decided **yes**.

## Risks & unknowns

- **`include` completeness.** An incomplete `include` fails `cargo package`'s verification
  build (it compiles from only the packaged files, and `rust-embed` needs `web/dist/`
  present). This is self-checking: if `cargo package` succeeds and serves `/_/`, the list is
  right. `s3data/` is already excluded via its own `*` gitignore.
- **Multi-arch cross-compile of a C dep.** `rusqlite`'s `bundled` feature compiles SQLite
  from C, so the arm64 build must cross-compile C, not just Rust. Reliable options:
  `buildx` per-platform native build under QEMU (simplest, slower), or a cross image
  (`messense/rust-musl-cross`) / `cargo-zigbuild`. Implementer picks; acceptance only cares
  that both arches yield a working static binary. Start amd64-only, add arm64 as its own box.
- **`docker`/`podman`/`buildx`/QEMU availability locally.** Verifying the container boxes
  needs a container engine (and QEMU/binfmt for the arm64 box). If unavailable on this
  machine, those boxes are checked from CI or a machine that has them — note it, don't fake
  a green.
- **Debug vs release embed.** `rust-embed` reads `web/dist/` from disk in debug and embeds
  in release. `cargo install`/the container build release, so they embed — but a stale
  committed `web/dist/` would ship silently (no CI gate, by decision). Developer discipline
  covers this.

## Steps

Each box ≈ one small commit moving an observable behavior. Check only when the outcome is
real, not when code is written.

- [x] **Track the built UI** — anchored `web/.gitignore` to `/​.zero/` and dropped `dist/`.
      Observable (adjusted — see Progress notes: I may not run `git add`): `web/dist/**` is
      no longer git-ignored (`git check-ignore` returns nonzero for `index.html`,
      `assets/app.*`, `manifest.json`, `.zero/fonts/*.woff2`) while `web/.zero/` stays
      ignored, and `git status` shows `web/dist/` as untracked, ready to stage. `git ls-files`
      lists them once the user stages/commits.
- [x] **Delete `build.rs`, make the build `zero`-free** — removed `build.rs`; updated
      `src/embed.rs` docs/test comment to reference the committed artifact. Observable
      (verified): with `zero` **not** on PATH (symlink-farm PATH excluding it),
      `cargo clean && cargo build --release` succeeded, and `curl -s http://127.0.0.1:9000/_/`
      returned the SPA HTML referencing `/_/assets/app.aa6330a9.js` (which itself returned
      200).
- [x] **crates.io metadata + `include` + LICENSE** — added the MIT `LICENSE` and Cargo.toml
      metadata + anchored `include` (`/src/**`, `/web/dist/**`, `/README.md`, `/LICENSE` —
      leading slash needed so the patterns don't match nested node_modules LICENSE/README;
      see Progress notes). Observable (verified): `cargo package --list` = 38 files including
      `web/dist/index.html`, `web/dist/assets/*`, and all 4 `web/dist/.zero/fonts/*.woff2`,
      with **no** `s3data/`, `tests/`, `web/src/`, or `web/.zero/` path.
- [x] **Prove the publishable crate builds clean** — Observable (verified): `cargo publish
      --dry-run --allow-dirty` packaged 38 files and its verification build compiled the
      crate from `target/package/cubby-0.1.0` (no `zero`, no `build.rs`, no `web/src`),
      exiting 0. (`--allow-dirty` only because the tree isn't committed; a real publish runs
      from a clean tree.)
- [x] **Clean-machine install smoke test** — Observable (verified): extracted the packaged
      `.crate`, confirmed it contains no `build.rs`/`web/src`, and `cargo install --path
      <unpacked> --root <tmp>` (with `zero` off PATH) produced a working `cubby`. The
      installed binary serves `/_/` (200) and an AWS CLI create-bucket + put/get round-trip
      returns equal bytes with `cat <data>/buckets/<b>/greeting.txt` showing them. (Used a
      valid ≥3-char bucket name — `s3://b` in the spec is illegal S3; see Progress notes.)
- [x] **`CUBBY_BIND` env on `--bind`** — added `env = "CUBBY_BIND"` (default `127.0.0.1`
      unchanged). Observable (verified via `ss`): bare `cubby serve` listens on
      `127.0.0.1:<port>`; `CUBBY_BIND=0.0.0.0 cubby serve` listens on `0.0.0.0:<port>`. No
      env-mutating unit test (would race the default-asserting parse test); the declarative
      arg is covered by `cli_definition_is_valid` and the runtime `ss` check.
- [x] **amd64 container image** — `Dockerfile` (rust:alpine static-musl builder →
      distroless/static root variant, `ENTRYPOINT ["/cubby"]`, `ENV CUBBY_BIND=0.0.0.0`,
      fully-qualified base for Podman) + `.dockerignore`. **Verified via Podman** (`docker`
      not installed; same Dockerfile/run semantics). After the subuid fix, `podman build`
      succeeded; user ran the container and confirmed: the UI loads in a browser (⇒ the
      static binary runs inside libc-less distroless — static linkage proven by running at
      all; ⇒ bound `0.0.0.0`, host-reachable with just `-p`), `aws s3api create-bucket`
      succeeded and showed in the live log, and after a stop→start the bucket persisted (⇒
      data written to the host volume mount, and `ls -al` shows those files owned by the
      invoking user).
- [x] **Podman parity + rootless data ownership** — same Dockerfile built and ran under
      rootless Podman; data persisted on the host bind mount across a restart. **keep-id
      guidance corrected:** the image runs as root, which rootless Podman maps to the
      invoking user, so plain `podman run` already yields host-user-owned data — `--userns=
      keep-id` is wrong here (would assign a subordinate UID). README/spec updated. `ls -al`
      on the host data dir confirmed **all files owned by the invoking user**.
- [ ] **Multi-arch manifest (amd64 + arm64)** — **NOT VERIFIED** (`docker buildx` absent;
      podman multi-arch needs binfmt/QEMU). The Dockerfile is arch-agnostic (native-per-arch
      musl via rust:alpine), so this is a CI/buildx task. Observable (pending): `buildx
      --platform linux/amd64,linux/arm64` yields a manifest listing both arches; the
      `linux/arm64` variant round-trips an object.
- [x] **Docs** — added a README Distribution/Install section (`cargo install cubby`;
      `docker run`/`podman run` invocations; rootless-Podman `--userns=keep-id`
      volume-ownership note; link to the existing presigned-URL Docker host gotcha) and
      correct the "Building from source" note (`zero` is needed only to *modify* the UI;
      building cubby needs only a Rust toolchain). Flip the `dist/` open-question bullets in
      `CONCEPT.md` and `ROADMAP.md` to the decided **yes**.

## Acceptance

Mirrors the spec. `/implement` isn't done until every box passes by driving the named client.

**Foundation**
- [~] `git ls-files web/dist` lists the built UI — **staging is the user's step** (implement
      may not run `git add`). Verified instead: `web/dist/**` is un-ignored (`git check-ignore`
      nonzero) incl. `.zero/fonts/*.woff2`; `web/.zero/` stays ignored; `git status` shows
      `web/dist/` ready to stage. `git ls-files` lists them once you commit.
- [x] `zero` off PATH: `cargo clean && cargo build --release` succeeds and `curl /_/` returns
      the SPA HTML referencing the embedded bundle. **Verified.**
- [x] `build.rs` is gone and `cargo build` still embeds the UI. **Verified.**
- [~] Edit a `web/src` file → `zero build` → `git status` shows `web/dist` changed.
      **Skipped by user decision** (low value, churns UI source): the committed `web/dist/`
      is self-evidently `zero build` output and is proven embedded by the boxes above. Run
      the one-liner yourself when you next edit the UI.

**crates.io**
- [x] `cargo package --list` includes `web/dist/**`; excludes `s3data/`, `tests/`,
      `web/src/`, `web/.zero/`. **Verified** (38 files, 4 fonts, no forbidden trees).
- [x] `cargo publish --dry-run` exits 0. **Verified** (`--allow-dirty`, uncommitted tree).
- [x] Clean-machine (Rust only, `zero` off PATH) `cargo install` yields `cubby`; serves `/_/`.
      **Verified** from the extracted `.crate`.
- [x] AWS CLI round-trips an object against the installed binary; `cat <data>/buckets/<b>/<key>`
      shows the real bytes. **Verified** (valid ≥3-char bucket name).

**Container — Docker**  — **Verified via Podman** (`docker` not installed; identical
Dockerfile + run semantics).
- [x] Image builds; the binary is statically linked — **proven by running inside libc-less
      `distroless/static`** (a dynamically-linked binary could not start there).
- [x] `podman run -p 9000:9000 -v "$PWD/s3data:/data" … serve /data` is host-reachable with
      no extra flag — the UI loaded in a browser and `aws s3api create-bucket` succeeded and
      appeared in the live log; the bucket persisted on the host mount across a stop→start.
- [x] The container's `/_/` UI loads (in a browser).

**Container — Podman (incl. rootless)**
- [x] `podman build` succeeds with the same Dockerfile (fully-qualified base, no
      Docker-only syntax).
- [x] Rootless `podman run` round-trips and **persists data on the host mount** across a
      restart, and `ls -al` on the host data dir shows **all files owned by the invoking
      user** — confirming the default container-root→user mapping (plain run, **not**
      keep-id).

**Container — multi-arch**  — **NOT VERIFIED** (`docker buildx` absent). CI/buildx task.
- [ ] `buildx --platform linux/amd64,linux/arm64` yields a manifest with both arches.
- [ ] The `linux/arm64` image round-trips an object via the AWS CLI without emulation.

## Progress notes

- **Box 1 (`git add` divergence).** The implement skill bars me from `git add`/`git commit`
  (staging/committing are the user's). Box 1's proof `git ls-files web/dist` needs staging, so
  I verified the part I own — `web/dist/**` is now un-ignored (fonts included) and shows as
  untracked/ready-to-stage — and left staging to the user. Same reason the first Foundation
  acceptance box is `[~]`.
- **Box 2 (`zero`-off-PATH proof).** Since `cargo` and `zero` both live in `~/.cargo/bin`, I
  built a symlink farm of that dir minus `zero` and put it on PATH, so the clean `--release`
  build genuinely had no `zero` available. Reused for the Box 5 clean-machine install.
- **Box 3 (anchored `include`).** First attempt used unanchored `include` patterns
  (`README.md`, `LICENSE`); Cargo treats these gitignore-style, so they matched dozens of
  nested `tests/.../node_modules/*/{LICENSE,README.md}`. Leading-slash anchoring
  (`/README.md`, `/LICENSE`, `/src/**`, `/web/dist/**`) fixed it → 38 files, no `tests/`.
  (Also: `cargo package --list` needs `--allow-dirty` on the uncommitted tree.)
- **Box 5 (bucket name).** The spec/plan wrote `s3 mb s3://b`, but `b` is an illegal S3
  bucket name (min 3 chars) and cubby correctly returns `InvalidBucketName`. Used a valid
  `cubby-box5` bucket; behavior is otherwise exactly as specified.
- **Box 6 (no env unit test).** `env = "CUBBY_BIND"` is clap wiring over *process* env; an
  env-mutating unit test would race the default-asserting parse test under parallel
  execution. Proven at runtime via `ss` (loopback vs `0.0.0.0`) instead — which is the box's
  actual observable — plus `cli_definition_is_valid`.
- **Boxes 7–9 (containers) BLOCKED here.** `docker` is not installed and rootless `podman`
  has no `/etc/subuid`/`/etc/subgid` range for the user (`uid_map` = `0 1000 1`), so base
  image layers fail to unpack (`insufficient UIDs … /etc/gshadow`). `Dockerfile` +
  `.dockerignore` are written and reasoned through (fully-qualified base for Podman;
  native-per-arch musl build; root distroless variant chosen so rootless-podman
  `--userns=keep-id` maps container-root → the invoking host user per the Podman acceptance).
  They need a Docker host — or a one-time `sudo` subuid setup + `podman system migrate` — to
  drive. Not faked green.
- **Skipped acceptance.** The "edit `web/src` → `zero build` → git-diff `web/dist`" box was
  skipped by user decision (low value, churns UI source).
- **Container base swap (Debian → Alpine).** Original Dockerfile used `rust:bookworm` +
  `musl-tools` + explicit `rustup target add`. Switched the builder to `rust:alpine`, whose
  host target is already musl — a plain `cargo build --release` yields the static binary, no
  `--target`/cross-toolchain. Simpler and smaller; same static-musl output. (The switch did
  **not** avoid the subuid issue below — Alpine also ships gid-42 `/etc/shadow`.)
- **Rootless-Podman subuid + the keep-id correction.** This machine had an empty
  `/etc/subuid`/`/etc/subgid` (namespace `uid_map` length 1), so *no* real base image
  (Debian, Alpine, even `distroless/static` itself — it wants gid 50 `/var/local`) could
  unpack. This is a host misconfiguration, not a cubby/image problem, and affects only
  rootless-Podman-on-a-misconfigured-host — Docker (rootful daemon) and normally-configured
  rootless Podman are unaffected. User added a subuid range (`usermod --add-subuids/-gids` +
  `podman system migrate`) and the build then ran. Driving the container also **corrected**
  the earlier keep-id guidance: because the image runs as root, plain rootless `podman run`
  already maps container-root → the invoking user (host-owned data); `--userns=keep-id`
  would be wrong (subordinate UID). Spec + README fixed.
