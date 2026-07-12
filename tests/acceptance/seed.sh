#!/usr/bin/env bash
#
# Phase 6 acceptance (--seed): drive the real cubby binary + AWS CLI + filesystem
# to prove the seed acceptance criteria of docs/features/06-seed-conformance-matrix.
# The inner loop is the seed_* integration tests in tests/s3_api.rs.
#
# Usage:  ./tests/acceptance/seed.sh
# Requires: aws (v2), curl, cmp, md5sum. Exits non-zero on any failed check.

set -uo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
BIN="$ROOT/target/debug/cubby"
if [ ! -x "$BIN" ]; then
  echo "building cubby…"
  (cd "$ROOT" && cargo build) || exit 1
fi
for tool in aws curl cmp md5sum; do
  command -v "$tool" >/dev/null || { echo "missing required tool: $tool"; exit 1; }
done

WORK=$(mktemp -d)
export AWS_ACCESS_KEY_ID=local
export AWS_SECRET_ACCESS_KEY=localsecret
export AWS_DEFAULT_REGION=us-east-1
export AWS_EC2_METADATA_DISABLED=true

PASS=0; FAIL=0
ok()  { echo "PASS: $1"; PASS=$((PASS+1)); }
bad() { echo "FAIL: $1"; FAIL=$((FAIL+1)); }

SRV=""
cleanup() { [ -n "$SRV" ] && kill "$SRV" 2>/dev/null; rm -rf "$WORK"; }
trap cleanup EXIT

# Start cubby on an ephemeral port; sets EP. Args after the datadir are passed
# through (e.g. --seed …). Returns non-zero if the server never prints a banner.
start_server() {
  local datadir="$1"; shift
  "$BIN" serve "$datadir" --port 0 --quiet "$@" >"$WORK/server.log" 2>&1 &
  SRV=$!
  EP=""
  for _ in $(seq 1 50); do
    local addr
    addr=$(grep -oP 'S3 API   → http://\K[0-9.]+:[0-9]+' "$WORK/server.log" | head -1)
    [ -n "$addr" ] && { EP="http://$addr"; return 0; }
    kill -0 "$SRV" 2>/dev/null || return 1
    sleep 0.1
  done
  return 1
}
stop_server() { [ -n "$SRV" ] && kill "$SRV" 2>/dev/null; wait "$SRV" 2>/dev/null; SRV=""; }

awss3()  { aws --endpoint-url "$EP" s3 "$@"; }
awsapi() { aws --endpoint-url "$EP" s3api "$@"; }

echo "=============== --seed (committed seed.yaml) ==============="

DATA1="$WORK/data1"
# The committed seed's file: path is relative to the seed file's dir (repo root).
if ! start_server "$DATA1" --seed "$ROOT/seed.yaml"; then
  echo "server did not start with --seed"; cat "$WORK/server.log"; exit 1
fi
echo "server at $EP"

# 1) Buckets appear (S3 + filesystem).
LS=$(awss3 ls)
echo "$LS" | grep -q ' uploads$' && echo "$LS" | grep -q ' reports$' \
  && ok "aws s3 ls shows uploads and reports" || { bad "aws s3 ls missing buckets"; echo "$LS"; }
[ -d "$DATA1/buckets/uploads" ] && [ -d "$DATA1/buckets/reports" ] \
  && ok "buckets/uploads and buckets/reports exist on disk" || bad "bucket dirs missing on disk"

# 2) Inline fixture is a real file with the right bytes + MD5 ETag.
DISK=$(cat "$DATA1/buckets/uploads/hello.txt")
[ "$DISK" = "hi there" ] && ok "cat buckets/uploads/hello.txt prints 'hi there'" || bad "on-disk hello.txt wrong: $DISK"
GOT=$(awss3 cp s3://uploads/hello.txt -)
[ "$GOT" = "hi there" ] && ok "aws s3 cp s3://uploads/hello.txt - returns the bytes" || bad "cp returned: $GOT"
WANT_ETAG=$(printf 'hi there\n' | md5sum | cut -d' ' -f1)
ETAG=$(awsapi head-object --bucket uploads --key hello.txt --query ETag --output text | tr -d '"')
[ "$ETAG" = "$WANT_ETAG" ] && ok "hello.txt ETag == MD5 of 'hi there\\n'" || bad "ETag $ETAG != $WANT_ETAG"

# 3) File-backed fixture loads real bytes.
cmp -s "$DATA1/buckets/uploads/photos/logo.png" "$ROOT/tests/fixtures/logo.png" \
  && ok "cmp buckets/uploads/photos/logo.png vs fixture is clean" || bad "logo.png on-disk cmp failed"
awsapi get-object --bucket uploads --key photos/logo.png "$WORK/logo.dl" >/dev/null
cmp -s "$WORK/logo.dl" "$ROOT/tests/fixtures/logo.png" \
  && ok "get-object photos/logo.png returns the fixture bytes" || bad "get-object logo.png bytes differ"

# 4) content_type + metadata applied.
CT=$(awsapi head-object --bucket uploads --key hello.txt --query ContentType --output text)
[ "$CT" = "text/plain" ] && ok "head-object ContentType == text/plain" || bad "ContentType: $CT"
TEAM=$(awsapi head-object --bucket uploads --key hello.txt --query 'Metadata.team' --output text)
[ "$TEAM" = "platform" ] && ok "head-object Metadata.team == platform" || bad "Metadata.team: $TEAM"

# 7) No --seed, no change: a fresh dir with no flag is empty.
stop_server
DATA_EMPTY="$WORK/empty"
start_server "$DATA_EMPTY"
LS=$(awss3 ls)
[ -z "$LS" ] && ok "no --seed: aws s3 ls on a fresh dir is empty" || { bad "fresh dir not empty"; echo "$LS"; }
stop_server

echo "=============== --seed idempotent / declarative ==============="

# 5) Re-run is idempotent + declarative (two serves against one persistent dir).
DATA2="$WORK/data2"
SEED="$WORK/seed.yaml"
printf 'buckets:\n  - name: uploads\n    objects:\n      - key: hello.txt\n        content: "one"\n' > "$SEED"

start_server "$DATA2" --seed "$SEED"
FIRST=$(awss3 cp s3://uploads/hello.txt -)
[ "$FIRST" = "one" ] && ok "first serve seeds hello.txt=one" || bad "first serve: $FIRST"
# A key created out-of-band, not named by the seed.
printf 'manual-bytes' > "$WORK/manual"
awss3 cp "$WORK/manual" s3://uploads/manual.txt >/dev/null
stop_server

# Re-serve the SAME dir with an edited seed: no "bucket exists" error, the named
# key is overwritten, and the out-of-band key survives.
printf 'buckets:\n  - name: uploads\n    objects:\n      - key: hello.txt\n        content: "two"\n' > "$SEED"
if start_server "$DATA2" --seed "$SEED"; then
  ok "re-serving the same dir with an existing bucket succeeds (idempotent)"
else
  bad "re-serve failed"; cat "$WORK/server.log"
fi
SECOND=$(awss3 cp s3://uploads/hello.txt -)
[ "$SECOND" = "two" ] && ok "edited inline content overwrites: hello.txt=two" || bad "re-serve content: $SECOND"
SURV=$(awss3 cp s3://uploads/manual.txt -)
[ "$SURV" = "manual-bytes" ] && ok "out-of-band key survives (not in seed, untouched)" || bad "manual.txt: $SURV"
stop_server

echo "=============== --seed fails fast, no bind ==============="

# 6) Malformed seed → non-zero exit, naming error, nothing listening.
BADSEED="$WORK/bad.yaml"
printf 'buckets:\n  - name: [not, valid\n' > "$BADSEED"
# Use a fixed, currently-free port so we can prove nothing is listening after.
PROBE=$(python3 - <<'PY'
import socket
s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()
PY
)
"$BIN" serve "$WORK/bad_data" --port "$PROBE" --seed "$BADSEED" >"$WORK/bad.log" 2>&1
RC=$?
[ "$RC" -ne 0 ] && ok "malformed seed exits non-zero (rc=$RC)" || bad "malformed seed exited 0"
grep -qi 'seed\|yaml' "$WORK/bad.log" && ok "error names the seed/YAML problem" || { bad "no naming error"; cat "$WORK/bad.log"; }
# Nothing is listening on the port — a connect is refused.
if curl -s -o /dev/null --max-time 2 "http://127.0.0.1:$PROBE/"; then
  bad "port $PROBE is listening after a failed seed"
else
  ok "nothing listening on port $PROBE (connection refused)"
fi

echo "======================================"
echo "PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ]
