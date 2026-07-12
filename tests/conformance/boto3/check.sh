#!/usr/bin/env bash
#
# boto3 conformance harness entry point. Ensures boto3 is importable (bootstrapping
# a throwaway venv via `uv` locally if needed), then runs the three checks.
# In CI, boto3 is pre-installed so `import boto3` succeeds and we use python3 directly.

set -uo pipefail
HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

PY=python3
if ! python3 -c 'import boto3' 2>/dev/null; then
  if command -v uv >/dev/null; then
    echo "bootstrapping boto3 into a throwaway venv via uv…" >&2
    uv venv "$CUBBY_WORK/venv" >/dev/null 2>&1 || { echo "uv venv failed" >&2; exit 1; }
    VIRTUAL_ENV="$CUBBY_WORK/venv" uv pip install --python "$CUBBY_WORK/venv/bin/python" boto3 \
      >/dev/null 2>&1 || { echo "uv pip install boto3 failed" >&2; exit 1; }
    PY="$CUBBY_WORK/venv/bin/python"
  elif [ "${CONFORMANCE_STRICT:-0}" = "1" ]; then
    echo "FAIL: boto3 not importable and uv not found (CONFORMANCE_STRICT=1)" >&2
    exit 1
  else
    echo "SKIP: boto3 not importable and uv not found" >&2
    exit 0
  fi
fi

exec "$PY" "$HERE/check.py"
