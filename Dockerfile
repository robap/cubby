# cubby — multi-stage build to a tiny, static, distroless image.
#
# Stage 1 compiles a fully *static* musl binary (no glibc, no shared libs), so
# stage 2 can be an almost-empty base. The build embeds the committed web UI
# (web/dist/, see src/embed.rs) — `zero` is never on the build path, honoring
# CONCEPT principle #5 ("builds with a Rust toolchain alone").
#
# The builder is Alpine-based: rust:alpine's default host target already IS
# *-unknown-linux-musl, so a plain `cargo build --release` produces a static
# binary — and per platform natively (under QEMU when you `buildx --platform
# linux/arm64` from an amd64 host), so amd64 and arm64 share one Dockerfile with
# no cross-compilers.

# ---- Stage 1: static musl build ------------------------------------------
# Fully qualified (docker.io/library/…) so it resolves identically under Docker
# and Podman — Podman does not assume Docker Hub for short names.
FROM docker.io/library/rust:alpine AS builder

# gcc + musl-dev (via build-base): libsqlite3-sys (rusqlite `bundled`) compiles
# SQLite from C and needs a C toolchain. Alpine's is musl-native, matching the
# static target.
RUN apk add --no-cache build-base

WORKDIR /src
COPY . .

# rust:alpine's host target is already *-linux-musl, so this is a static build
# for the current platform — no --target and no cross toolchain needed. The
# binary lands at target/release/cubby.
RUN cargo build --release --locked --bin cubby \
 && cp target/release/cubby /cubby

# ---- Stage 2: distroless runtime -----------------------------------------
# static-debian12 (root variant, on purpose): ships CA certs (ready for v0.2
# webhooks) and no shell/package manager. Root default so `podman run
# --userns=keep-id` maps container-root to the invoking host user, leaving
# bind-mounted data files owned by *you* (see README). Run non-root any time
# with `--user`.
FROM gcr.io/distroless/static-debian12

COPY --from=builder /cubby /cubby

# Reachable from the host: 127.0.0.1 would be unreachable across the container
# boundary. Set via env (not CMD args) so it survives `docker run … serve /data`
# overriding the default command. An explicit `--bind` still wins.
ENV CUBBY_BIND=0.0.0.0
EXPOSE 9000

ENTRYPOINT ["/cubby"]
CMD ["serve", "/data"]
