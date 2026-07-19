#!/usr/bin/env bash
#
# v0.2 acceptance (reindex): drive the real cubby binary + AWS CLI + filesystem
# to prove the reindex acceptance criteria of docs/features/reindex-spec.md.
# reindex's only output is SQLite state, so every criterion is proven by
# reindexing a byte tree, then `cubby serve`-ing it and asking a real client.
# The inner loop is the tests in tests/reindex.rs.
#
# Usage:  ./tests/acceptance/reindex.sh
# Requires: aws (v2), curl, cmp, md5sum, stat. Exits non-zero on any failed check.

set -uo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
BIN="$ROOT/target/debug/cubby"
if [ ! -x "$BIN" ]; then
  echo "building cubby…"
  (cd "$ROOT" && cargo build) || exit 1
fi
for tool in aws curl cmp md5sum stat; do
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

# Start cubby on an ephemeral port; sets EP. Returns non-zero if no banner.
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

# Offline reindex; captures its summary to $WORK/reindex.log.
reindex() { "$BIN" reindex "$1" >"$WORK/reindex.log" 2>&1; }

awss3()  { aws --endpoint-url "$EP" s3 "$@"; }
awsapi() { aws --endpoint-url "$EP" s3api "$@"; }

echo "=============== reindex a hand-built byte tree ==============="

DATA="$WORK/data"
# Hand-build the tree with NO meta.sqlite / no rows — pure dropped-in bytes.
mkdir -p "$DATA/buckets/uploads/photos" "$DATA/buckets/newbucket"
printf 'the quarterly report\n'      > "$DATA/buckets/uploads/report.pdf"
printf 'pretend jpeg bytes\n'        > "$DATA/buckets/uploads/photos/cat.jpg"
printf 'just some notes\n'           > "$DATA/buckets/uploads/notes.txt"
printf 'hand-made bucket object\n'   > "$DATA/buckets/newbucket/x"
# Internal trees that must be ignored: a stray file in .tmp/ and a dir in
# .multipart/ (created before reindex; reindex's bootstrap won't disturb them).
mkdir -p "$DATA/.tmp" "$DATA/.multipart/upload-xyz"
printf 'staging junk\n' > "$DATA/.tmp/stray.txt"

if reindex "$DATA"; then
  ok "cubby reindex exits 0 on a fresh byte tree"
else
  bad "cubby reindex failed"; cat "$WORK/reindex.log"
fi
cat "$WORK/reindex.log"

start_server "$DATA" || { echo "server did not start"; cat "$WORK/server.log"; exit 1; }
echo "server at $EP"

# 1) Loose file becomes a listable, downloadable object.
LS=$(awss3 ls s3://uploads/)
echo "$LS" | grep -q 'report.pdf' && ok "aws s3 ls s3://uploads/ lists report.pdf" || { bad "report.pdf not listed"; echo "$LS"; }
GOT=$(awss3 cp s3://uploads/report.pdf -)
[ "$GOT" = "the quarterly report" ] && ok "aws s3 cp s3://uploads/report.pdf - returns the bytes" || bad "cp returned: $GOT"

# 2) ETag == content-MD5, ContentLength == byte size.
WANT_ETAG=$(md5sum "$DATA/buckets/uploads/report.pdf" | cut -d' ' -f1)
WANT_LEN=$(stat -c%s "$DATA/buckets/uploads/report.pdf")
ETAG=$(awsapi head-object --bucket uploads --key report.pdf --query ETag --output text | tr -d '"')
LEN=$(awsapi head-object --bucket uploads --key report.pdf --query ContentLength --output text)
[ "$ETAG" = "$WANT_ETAG" ] && ok "report.pdf ETag == content-MD5" || bad "ETag $ETAG != $WANT_ETAG"
[ "$LEN" = "$WANT_LEN" ]   && ok "report.pdf ContentLength == byte size ($WANT_LEN)" || bad "ContentLength $LEN != $WANT_LEN"

# 3) Hand-made bucket directory adopted.
awss3 ls | grep -q ' newbucket$' && ok "aws s3 ls lists the adopted newbucket" || { bad "newbucket not listed"; awss3 ls; }
awss3 ls s3://newbucket/ | grep -q ' x$' && ok "aws s3 ls s3://newbucket/ shows its object" || bad "newbucket object missing"

# 4) Nested prefixes recover as keys.
awss3 ls s3://uploads/ --recursive | grep -q 'photos/cat.jpg' \
  && ok "recursive ls shows photos/cat.jpg" || bad "nested key not recovered"
awss3 ls s3://uploads/photos/ | grep -q 'cat.jpg' \
  && ok "aws s3 ls s3://uploads/photos/ shows cat.jpg under the prefix" || bad "delimited prefix listing missing cat.jpg"

# 7) content_type guessed; metadata absent.
CT=$(awsapi head-object --bucket uploads --key notes.txt --query ContentType --output text)
[ "$CT" = "text/plain" ] && ok "notes.txt ContentType guessed as text/plain" || bad "ContentType: $CT"
# A `--query Metadata` on an empty map prints nothing (a CLI quirk), so assert on
# the full head-object JSON, which renders the empty map as `"Metadata": {}`.
HEAD=$(awsapi head-object --bucket uploads --key notes.txt)
echo "$HEAD" | grep -q '"Metadata": {}' && ok "notes.txt Metadata is an empty map" || { bad "Metadata not empty"; echo "$HEAD"; }

# 9) Internal trees ignored — only what's under buckets/ appears.
BUCKETS=$(awss3 ls | awk '{print $NF}' | sort | tr '\n' ' ')
[ "$BUCKETS" = "newbucket uploads " ] && ok "only buckets/ dirs are buckets (.tmp/.multipart ignored)" || bad "unexpected buckets: $BUCKETS"
awss3 ls s3://uploads/ --recursive | grep -q 'stray.txt' && bad "a .tmp stray file leaked into an object" || ok "no .tmp/.multipart entries indexed"

echo "=============== full rebuild from bytes alone ==============="

# 5) Populated dir → rm meta.sqlite → reindex → serve reproduces everything.
stop_server
# Record what the bytes say before we throw the index away.
declare -A WANT_MD5
for f in uploads/report.pdf uploads/photos/cat.jpg uploads/notes.txt newbucket/x; do
  WANT_MD5[$f]=$(md5sum "$DATA/buckets/$f" | cut -d' ' -f1)
done
rm -f "$DATA/meta.sqlite" "$DATA/meta.sqlite-wal" "$DATA/meta.sqlite-shm"
reindex "$DATA" && ok "reindex after rm meta.sqlite exits 0" || { bad "rebuild reindex failed"; cat "$WORK/reindex.log"; }
start_server "$DATA" || { echo "server did not restart"; exit 1; }

REBUILD_OK=1
for f in uploads/report.pdf uploads/photos/cat.jpg uploads/notes.txt newbucket/x; do
  b=${f%%/*}; k=${f#*/}
  et=$(awsapi head-object --bucket "$b" --key "$k" --query ETag --output text 2>/dev/null | tr -d '"')
  [ "$et" = "${WANT_MD5[$f]}" ] || { REBUILD_OK=0; echo "  $f: ETag $et != ${WANT_MD5[$f]}"; }
done
[ "$REBUILD_OK" = 1 ] && ok "every bucket/object rebuilt with single-part ETags matching the bytes" || bad "rebuild mismatch"
stop_server

echo "=============== special-character key round-trip ==============="

# 6) A `:` key PUT by a client survives a lost meta.sqlite via percent-decode.
DATA2="$WORK/data2"
start_server "$DATA2" || { echo "server did not start"; exit 1; }
awsapi create-bucket --bucket uploads >/dev/null
printf 'colon key bytes\n' > "$WORK/weird.src"
awsapi put-object --bucket uploads --key 'weird:name.txt' --body "$WORK/weird.src" >/dev/null
# On disk the colon is percent-encoded.
[ -f "$DATA2/buckets/uploads/weird%3Aname.txt" ] \
  && ok "colon key stored on disk as weird%3Aname.txt" || bad "encoded filename missing on disk"
WANT_LEN=$(stat -c%s "$WORK/weird.src")
stop_server
rm -f "$DATA2/meta.sqlite" "$DATA2/meta.sqlite-wal" "$DATA2/meta.sqlite-shm"
reindex "$DATA2" || { bad "reindex of colon-key dir failed"; cat "$WORK/reindex.log"; }
start_server "$DATA2" || { echo "server did not restart"; exit 1; }
# The colon key is recovered exactly — head-object by the original key succeeds.
LEN=$(awsapi head-object --bucket uploads --key 'weird:name.txt' --query ContentLength --output text 2>/dev/null)
[ "$LEN" = "$WANT_LEN" ] && ok "head-object 'weird:name.txt' returns 200 with the right size after rebuild" \
  || bad "colon key not recovered (len=$LEN, want $WANT_LEN)"
stop_server

echo "=============== idempotent, non-destructive re-run ==============="

# 8) Two reindex runs: the second indexes 0; a pre-existing custom row survives.
DATA3="$WORK/data3"
start_server "$DATA3" || { echo "server did not start"; exit 1; }
awsapi create-bucket --bucket uploads >/dev/null
printf 'kept bytes\n' > "$WORK/keep.src"
awsapi put-object --bucket uploads --key keep.txt \
  --content-type application/x-custom --metadata k=v --body "$WORK/keep.src" >/dev/null
stop_server
# Drop one new file by hand so the first run has something to index.
printf 'fresh\n' > "$DATA3/buckets/uploads/fresh.txt"

reindex "$DATA3"; FIRST=$(grep -o 'objects: [0-9]* indexed' "$WORK/reindex.log")
echo "  first run: $FIRST"
echo "$FIRST" | grep -q 'objects: 1 indexed' && ok "first re-run indexes the one new file" || bad "first run: $FIRST"
reindex "$DATA3"; SECOND=$(grep -o 'objects: [0-9]* indexed' "$WORK/reindex.log")
echo "  second run: $SECOND"
echo "$SECOND" | grep -q 'objects: 0 indexed' && ok "second re-run indexes 0 objects (idempotent)" || bad "second run: $SECOND"

start_server "$DATA3" || { echo "server did not restart"; exit 1; }
CT=$(awsapi head-object --bucket uploads --key keep.txt --query ContentType --output text)
KV=$(awsapi head-object --bucket uploads --key keep.txt --query 'Metadata.k' --output text)
[ "$CT" = "application/x-custom" ] && ok "custom content-type survives both re-runs" || bad "ContentType: $CT"
[ "$KV" = "v" ] && ok "user metadata k=v survives both re-runs" || bad "Metadata.k: $KV"
stop_server

echo "======================================"
echo "PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ]
