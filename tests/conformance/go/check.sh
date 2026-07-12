#!/usr/bin/env bash
#
# aws-sdk-go-v2 harness entry point. Builds and runs the small Go program; the
# committed go.mod/go.sum pin the SDK versions for reproducibility.

set -uo pipefail
HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

if ! command -v go >/dev/null; then
  [ "${CONFORMANCE_STRICT:-0}" = "1" ] && { echo "FAIL: go not found" >&2; exit 1; }
  echo "SKIP: go not found" >&2
  exit 0
fi

cd "$HERE"
exec go run .
