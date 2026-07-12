#!/usr/bin/env bash
#
# Shared conformance runner. Starts a fresh cubby on an ephemeral port against an
# isolated temp data dir, generates the shared >8MB test object once, then
# dispatches to a single client harness which runs the three conformance checks:
#
#   1. round-trip incl. a >8MB multipart upload (bytes verified equal)
#   2. list a nested layout with prefix + `/` delimiter (keys + CommonPrefixes)
#   3. presigned GET fetched with no ambient credentials
#
# Usage:  ./tests/conformance/run.sh <boto3|awscli|js|go|rclone>
#
# This is the local driver for docs/features/06-seed-conformance-matrix — the
# same script each GitHub Actions matrix job invokes with its client name. It
# extends the tests/acceptance/*.sh convention (start cubby --port 0, parse the
# machine-parseable banner line for the address).
#
# Missing-toolchain policy: locally, a client whose toolchain is absent is
# warn-and-SKIPPED (exit 0) so a dev without go/rclone can still run the rest.
# In CI, export CONFORMANCE_STRICT=1 so a missing toolchain FAILS the job — a
# real regression must never hide behind a green skip.

set -uo pipefail

CLIENT="${1:-}"
if [ -z "$CLIENT" ]; then
  echo "usage: $0 <boto3|awscli|js|go|rclone>" >&2
  exit 2
fi

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
HERE="$ROOT/tests/conformance"
BIN="$ROOT/target/debug/cubby"

STRICT="${CONFORMANCE_STRICT:-0}"

# Warn-and-skip (local) vs fail (CI) when a required toolchain is missing.
missing_tool() {
  local tool="$1"
  if [ "$STRICT" = "1" ]; then
    echo "FAIL: required tool '$tool' not found (CONFORMANCE_STRICT=1)" >&2
    exit 1
  fi
  echo "SKIP: '$tool' not found; skipping $CLIENT conformance (set CONFORMANCE_STRICT=1 to fail)" >&2
  exit 0
}

# --- build cubby if needed --------------------------------------------------
if [ ! -x "$BIN" ]; then
  echo "building cubby…"
  (cd "$ROOT" && cargo build) || exit 1
fi

# --- per-client toolchain presence -----------------------------------------
case "$CLIENT" in
  boto3)  command -v python3 >/dev/null || missing_tool python3 ;;
  awscli) command -v aws     >/dev/null || missing_tool aws
          command -v curl    >/dev/null || missing_tool curl ;;
  js)     command -v node    >/dev/null || missing_tool node ;;
  go)     command -v go      >/dev/null || missing_tool go ;;
  rclone) command -v rclone  >/dev/null || missing_tool rclone
          command -v curl    >/dev/null || missing_tool curl ;;
  *) echo "unknown client: $CLIENT" >&2; exit 2 ;;
esac

# --- workspace + isolated data dir -----------------------------------------
WORK=$(mktemp -d)
DATADIR="$WORK/s3data"
cleanup() { [ -n "${SRV:-}" ] && kill "$SRV" 2>/dev/null; rm -rf "$WORK"; }
trap cleanup EXIT

# --- the shared >8MB object (generated at runtime, never committed) --------
# 12MB > the 8MB threshold at which every SDK auto-switches to multipart.
BIG="$WORK/big.bin"
head -c 12000000 /dev/urandom > "$BIG"

# --- credentials the fixed-key server accepts ------------------------------
export AWS_ACCESS_KEY_ID=local
export AWS_SECRET_ACCESS_KEY=localsecret
export AWS_DEFAULT_REGION=us-east-1
export AWS_REGION=us-east-1
export AWS_EC2_METADATA_DISABLED=true

# --- start the server on an ephemeral port ---------------------------------
"$BIN" serve "$DATADIR" --port 0 --quiet >"$WORK/server.log" 2>&1 &
SRV=$!

ADDR=""
for _ in $(seq 1 50); do
  ADDR=$(grep -oP 'S3 API   → http://\K[0-9.]+:[0-9]+' "$WORK/server.log" | head -1)
  [ -n "$ADDR" ] && break
  # bail early if the server died
  kill -0 "$SRV" 2>/dev/null || break
  sleep 0.1
done
if [ -z "$ADDR" ]; then
  echo "server did not start" >&2
  cat "$WORK/server.log" >&2
  exit 1
fi
EP="http://$ADDR"

echo "== conformance: $CLIENT =="
echo "server at $EP  (data dir $DATADIR)"

# --- environment the harnesses read ----------------------------------------
export CUBBY_EP="$EP"
export CUBBY_ADDR="$ADDR"
export CUBBY_BIG="$BIG"
export CUBBY_DATADIR="$DATADIR"
export CUBBY_WORK="$WORK"
# A bucket name unique per client so parallel local runs never collide.
export CUBBY_BUCKET="conf-$CLIENT"

# --- dispatch ---------------------------------------------------------------
case "$CLIENT" in
  boto3)  bash "$HERE/boto3/check.sh" ;;
  awscli) bash "$HERE/awscli/check.sh" ;;
  js)     bash "$HERE/js/check.sh" ;;
  go)     bash "$HERE/go/check.sh" ;;
  rclone) bash "$HERE/rclone/check.sh" ;;
esac
RC=$?

if [ "$RC" -eq 0 ]; then
  echo "PASS: $CLIENT conformance"
else
  echo "FAIL: $CLIENT conformance (rc=$RC)" >&2
fi
exit "$RC"
