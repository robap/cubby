# Release workflow

The deferred "release automation" from `docs/features/distribution-spec.md`. This
is a **reference draft** — review it, then save the YAML block below as
`.github/workflows/release.yml` to make it live.

## What it does

Fires when you push a version tag (`v0.1.1`) and, in parallel:

1. **Container image → ghcr.io** — builds the **multi-arch** (amd64 + arm64)
   image with `buildx` + QEMU and pushes `ghcr.io/robap/cubby:<version>` and
   `:latest`. This is also what verifies the arm64 build cubby couldn't build
   locally (GitHub runners have buildx/QEMU).
2. **crates.io** *(optional)* — `cargo publish`, gated behind a repo variable so
   it only runs once you've opted in and added a token.

Nothing is automated until this file exists on the default branch. Until then,
image builds and `cargo publish` remain manual.

## The workflow

```yaml
name: release

# Push a version tag to cut a release:
#   git tag v0.1.1 && git push origin v0.1.1
on:
  push:
    tags:
      - "v*.*.*"

permissions:
  contents: read
  packages: write # push the image to ghcr.io (GITHUB_TOKEN gets write here)

jobs:
  # --- Container image → ghcr.io (multi-arch) ------------------------------
  image:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      # arm64 emulation so buildx can build linux/arm64 on an amd64 runner.
      - uses: docker/setup-qemu-action@v3
      - uses: docker/setup-buildx-action@v3

      - name: Log in to ghcr.io
        uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      # Derives tags from the git tag: v0.1.1 -> :0.1.1 and :latest.
      - name: Image metadata
        id: meta
        uses: docker/metadata-action@v5
        with:
          images: ghcr.io/${{ github.repository }}
          tags: |
            type=semver,pattern={{version}}
            type=raw,value=latest

      - name: Build and push (amd64 + arm64)
        uses: docker/build-push-action@v6
        with:
          context: .
          platforms: linux/amd64,linux/arm64
          push: true
          tags: ${{ steps.meta.outputs.tags }}
          labels: ${{ steps.meta.outputs.labels }}
          # Reuse layers across releases so the in-container Rust build is fast.
          cache-from: type=gha
          cache-to: type=gha,mode=max

  # --- crates.io (opt-in) --------------------------------------------------
  crate:
    runs-on: ubuntu-latest
    # Set repo variable PUBLISH_TO_CRATES_IO=true and add the CARGO_REGISTRY_TOKEN
    # secret to enable. Otherwise this job is skipped.
    if: ${{ vars.PUBLISH_TO_CRATES_IO == 'true' }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable

      # Keep crates.io and the image tag from drifting: the pushed tag must equal
      # the Cargo.toml version (cargo publish uses Cargo.toml, not the tag).
      - name: Tag matches Cargo.toml version
        run: |
          set -euo pipefail
          crate_ver=$(cargo metadata --no-deps --format-version 1 \
            | jq -r '.packages[0].version')
          tag_ver="${GITHUB_REF_NAME#v}"
          echo "tag=$tag_ver  Cargo.toml=$crate_ver"
          test "$crate_ver" = "$tag_ver"

      # Verification build compiles the packaged crate (embeds web/dist/, no
      # `zero`). Fails loudly if the committed UI or the include list is off.
      - name: Publish to crates.io
        run: cargo publish --locked
        env:
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
```

## One-time setup

**ghcr.io (image job) — nothing to configure to *push*.** The built-in
`GITHUB_TOKEN` already has `packages: write` for your own repo. But the first
push creates a **private** package. To let people `docker pull` without auth:
GitHub → your profile → Packages → `cubby` → Package settings → set visibility
**Public**, and (optional) link it to the repo. Do this once.

**crates.io (crate job) — opt-in.** It stays skipped until you:

1. Create a crates.io API token and add it as the repo **secret**
   `CARGO_REGISTRY_TOKEN`.
2. Add the repo **variable** `PUBLISH_TO_CRATES_IO` = `true`.

The very first crate publish is best done **manually** anyway (`cargo publish`)
so you can eyeball the crates.io page; turn the variable on afterward for
hands-off releases.

## Cutting a release

```bash
# 1. If the UI changed, rebuild and commit it FIRST — there is no CI gate on
#    web/dist/ freshness, so a stale UI would ship silently.
zero build && git add web/dist

# 2. Bump the version so the tag, image tag, and crate version all agree.
#    (edit Cargo.toml: version = "0.1.1")
git commit -am "release: v0.1.1"

# 3. Tag and push — this triggers the workflow.
git tag v0.1.1
git push origin main v0.1.1
```

GitHub then builds the multi-arch image, pushes `:0.1.1` + `:latest` to
`ghcr.io/robap/cubby`, and (if enabled) publishes the crate.

## Notes

- **Version is the source of truth in three places at once.** The git tag, the
  image tag (derived from it), and `cargo publish` (from `Cargo.toml`). The
  `crate` job's guard enforces tag == `Cargo.toml`; keep them in lockstep when
  you bump.
- **UI freshness.** The single biggest footgun in this flow is publishing with a
  stale `web/dist/`. Step 1 above (`zero build` before tagging) is not optional —
  it is the manual replacement for the CI gate we deliberately don't have.
- **Cost / speed.** The image job compiles cubby twice (once per arch, arm64
  under emulation). The `gha` cache keeps repeat releases from recompiling
  unchanged dependency layers; the arm64 leg is still the slow one.
- **Scope.** This does not build GitHub-release binaries (cargo-dist) or a Homebrew
  tap — both remain separate follow-ups, unblocked by the committed `web/dist/`.
