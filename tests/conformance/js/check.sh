#!/usr/bin/env bash
#
# aws-sdk-js v3 harness entry point. Installs the SDK into this dir on first run
# (node_modules is gitignored), then runs the three checks with node's global fetch.

set -uo pipefail
HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

if ! command -v npm >/dev/null; then
  [ "${CONFORMANCE_STRICT:-0}" = "1" ] && { echo "FAIL: npm not found" >&2; exit 1; }
  echo "SKIP: npm not found" >&2
  exit 0
fi

if [ ! -d "$HERE/node_modules/@aws-sdk/client-s3" ]; then
  echo "installing aws-sdk-js v3 deps…" >&2
  if ! ( cd "$HERE" && npm install --no-audit --no-fund >/dev/null 2>&1 ); then
    [ "${CONFORMANCE_STRICT:-0}" = "1" ] && { echo "FAIL: npm install failed" >&2; exit 1; }
    echo "SKIP: npm install failed (offline?)" >&2
    exit 0
  fi
fi

exec node "$HERE/check.mjs"
