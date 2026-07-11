#!/usr/bin/env bash
#
# Phase 2 acceptance: drive the real rclone + AWS CLI against a live cubby
# server, exercising ListObjectsV2 and legacy ListObjects end to end. This is
# the outer verification loop for docs/features/02-listing-delimiters-spec.md —
# the aws-sdk-s3 integration tests in tests/s3_api.rs are the inner loop.
#
# Usage:  ./tests/acceptance/listing.sh
# Requires: rclone, aws (v2), python3, a Rust toolchain. Exits non-zero on any
# failed check.
#
# Notes on two client quirks handled below (the wire XML is correct — verify
# with `aws --debug`):
#   - botocore auto-paginates list-objects*, merging pages and dropping per-page
#     scalars like KeyCount/IsTruncated; field asserts pass --no-paginate.
#   - `rclone lsf -R` prints synthesised directory entries; the flat-keys check
#     passes --files-only.

set -uo pipefail

# --- locate (building if needed) the cubby binary -------------------------
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
BIN="$ROOT/target/debug/cubby"
if [ ! -x "$BIN" ]; then
  echo "building cubby…"
  (cd "$ROOT" && cargo build) || exit 1
fi

for tool in rclone aws python3; do
  command -v "$tool" >/dev/null || { echo "missing required tool: $tool"; exit 1; }
done

WORK=$(mktemp -d)
DATADIR="$WORK/s3data"
DB="$DATADIR/meta.sqlite"
export AWS_ACCESS_KEY_ID=local
export AWS_SECRET_ACCESS_KEY=localsecret
export AWS_DEFAULT_REGION=us-east-1
export AWS_EC2_METADATA_DISABLED=true

PASS=0; FAIL=0
ok()  { echo "PASS: $1"; PASS=$((PASS+1)); }
bad() { echo "FAIL: $1"; FAIL=$((FAIL+1)); }

# --- start the server on an ephemeral port ---------------------------------
"$BIN" serve "$DATADIR" --port 0 >"$WORK/server.log" 2>&1 &
SRV=$!
trap 'kill $SRV 2>/dev/null; rm -rf "$WORK"' EXIT

ADDR=""
for _ in $(seq 1 50); do
  ADDR=$(grep -oP 'S3 API   → http://\K[0-9.]+:[0-9]+' "$WORK/server.log" | head -1)
  [ -n "$ADDR" ] && break
  sleep 0.1
done
[ -z "$ADDR" ] && { echo "server did not start"; cat "$WORK/server.log"; exit 1; }
EP="http://$ADDR"
echo "server at $EP"

awss3()  { aws --endpoint-url "$EP" s3 "$@"; }
awsapi() { aws --endpoint-url "$EP" s3api "$@"; }

# rclone remote "r:" configured entirely via env (S3 provider "Other").
export RCLONE_CONFIG_R_TYPE=s3
export RCLONE_CONFIG_R_PROVIDER=Other
export RCLONE_CONFIG_R_ENV_AUTH=false
export RCLONE_CONFIG_R_ACCESS_KEY_ID=local
export RCLONE_CONFIG_R_SECRET_ACCESS_KEY=localsecret
export RCLONE_CONFIG_R_ENDPOINT=$EP
export RCLONE_CONFIG_R_REGION=us-east-1
export RCLONE_CONFIG_R_FORCE_PATH_STYLE=true

# Seed rows straight into SQLite (listing reads SQLite; fast for big fixtures).
seed_rows() { # seed_rows <bucket> <python-range-expr producing keys>
  python3 - "$DB" "$1" "$2" <<'PY'
import sys, sqlite3
db_path, bucket, expr = sys.argv[1], sys.argv[2], sys.argv[3]
keys = eval(expr)  # e.g. "[f'k{i:05}' for i in range(2500)]"
db = sqlite3.connect(db_path); db.execute("PRAGMA journal_mode=WAL")
db.executemany(
    "INSERT OR REPLACE INTO objects (bucket,key,size,etag,last_modified,metadata) "
    "VALUES (?,?,0,'d41d8cd98f00b204e9800998ecf8427e',0,'{}')",
    [(bucket, k) for k in keys],
)
db.commit(); db.close()
PY
}
jget() { python3 -c 'import sys,json;print(json.load(sys.stdin).get(sys.argv[1],""))' "$1"; }

# --- fixture bucket `photos` -----------------------------------------------
awsapi create-bucket --bucket photos >/dev/null
mkdir -p "$WORK/bodies"
for k in notes.txt photos/index.md photos/2024/a.jpg photos/2024/b.jpg photos/2025/c.jpg; do
  BF="$WORK/bodies/$(echo "$k" | tr '/' '_')"
  printf 'content-of-%s' "$k" > "$BF"
  awsapi put-object --bucket photos --key "$k" --body "$BF" >/dev/null
done

echo "=============== rclone ==============="

# rclone lsf remote:photos → notes.txt and photos/
OUT=$(rclone lsf r:photos 2>/dev/null | sort | tr '\n' ' ')
[ "$OUT" = "notes.txt photos/ " ] \
  && ok "rclone lsf top-level = notes.txt + photos/" || bad "rclone lsf top-level: '$OUT'"

# rclone lsf -R remote:photos → all five keys, flat, lexicographic
OUT=$(rclone lsf -R --files-only r:photos 2>/dev/null | sort | tr '\n' ' ')
EXP="notes.txt photos/2024/a.jpg photos/2024/b.jpg photos/2025/c.jpg photos/index.md "
[ "$OUT" = "$EXP" ] && ok "rclone lsf -R = all five flat" || bad "rclone lsf -R: '$OUT'"

# rclone sync → cmp clean, second sync transfers 0
DL="$WORK/dl"
if rclone sync r:photos "$DL" 2>"$WORK/sync1.log"; then
  CLEAN=1
  for k in notes.txt photos/index.md photos/2024/a.jpg photos/2024/b.jpg photos/2025/c.jpg; do
    [ -f "$DL/$k" ] && [ "$(cat "$DL/$k")" = "content-of-$k" ] || CLEAN=0
  done
  [ $CLEAN -eq 1 ] && ok "rclone sync downloaded every key, cmp clean" || bad "rclone sync content mismatch"
else
  bad "rclone sync exit nonzero"; cat "$WORK/sync1.log"
fi
rclone sync r:photos "$DL" 2>"$WORK/sync2.log"
if grep -qE 'Transferred:.*0 B|There was nothing to transfer' "$WORK/sync2.log" \
   || ! grep -q 'Copied' "$WORK/sync2.log"; then
  ok "second rclone sync transferred 0"
else
  bad "second rclone sync transferred something"; cat "$WORK/sync2.log"
fi

# rclone lsf remote:photos/photos/2024/ → a.jpg and b.jpg (nested traversal)
OUT=$(rclone lsf r:photos/photos/2024/ 2>/dev/null | sort | tr '\n' ' ')
[ "$OUT" = "a.jpg b.jpg " ] && ok "rclone nested-prefix lsf = a.jpg b.jpg" || bad "rclone nested lsf: '$OUT'"

echo "=============== aws cli ==============="

# aws s3 ls s3://photos/ → PRE photos/ + notes.txt
OUT=$(awss3 ls s3://photos/ 2>&1)
{ echo "$OUT" | grep -q "PRE photos/" && echo "$OUT" | grep -q "notes.txt"; } \
  && ok "aws s3 ls = PRE photos/ + notes.txt" || bad "aws s3 ls: $OUT"

# list-objects-v2 --prefix photos/ --delimiter / → CPs, Contents, KeyCount 3
J=$(awsapi list-objects-v2 --no-paginate --bucket photos --prefix photos/ --delimiter /)
CPS=$(echo "$J" | python3 -c 'import sys,json;d=json.load(sys.stdin);print(",".join(p["Prefix"] for p in d.get("CommonPrefixes",[])))')
KEYS=$(echo "$J" | python3 -c 'import sys,json;d=json.load(sys.stdin);print(",".join(o["Key"] for o in d.get("Contents",[])))')
KC=$(echo "$J" | jget KeyCount)
[ "$CPS" = "photos/2024/,photos/2025/" ] && [ "$KEYS" = "photos/index.md" ] && [ "$KC" = "3" ] \
  && ok "v2 prefix+delimiter grouping (KeyCount 3)" || bad "v2 grouping: CPs=$CPS Keys=$KEYS KC=$KC"

# --- 2500-key bucket -------------------------------------------------------
awsapi create-bucket --bucket paged >/dev/null
seed_rows paged "[f'k{i:05}' for i in range(2500)]"

# Follow continuation tokens by hand: 2500 keys, 3 pages, in order, no dupes.
TOTAL=0; TOKEN=""; PAGES=0; COLLECT="$WORK/collected"; : > "$COLLECT"
while :; do
  if [ -z "$TOKEN" ]; then
    J=$(awsapi list-objects-v2 --no-paginate --bucket paged --max-keys 1000)
  else
    J=$(awsapi list-objects-v2 --no-paginate --bucket paged --max-keys 1000 --continuation-token "$TOKEN")
  fi
  PAGES=$((PAGES+1))
  echo "$J" | python3 -c 'import sys,json;[print(o["Key"]) for o in json.load(sys.stdin).get("Contents",[])]' >> "$COLLECT"
  read -r CNT TRUNC NEXT < <(echo "$J" | python3 -c '
import sys,json
d=json.load(sys.stdin)
print(len(d.get("Contents",[])), str(d.get("IsTruncated")).lower(), d.get("NextContinuationToken",""))')
  TOTAL=$((TOTAL+CNT))
  if [ "$TRUNC" = "true" ]; then TOKEN="$NEXT"; else break; fi
done
UNIQ=$(sort -u "$COLLECT" | wc -l)
SORTED_OK=$(if [ "$(sort "$COLLECT")" = "$(cat "$COLLECT")" ]; then echo yes; else echo no; fi)
[ "$TOTAL" = "2500" ] && [ "$PAGES" = "3" ] && [ "$UNIQ" = "2500" ] && [ "$SORTED_OK" = "yes" ] \
  && ok "v2 pagination → 2500 keys over 3 pages, in order, no dupes" \
  || bad "v2 pagination: total=$TOTAL pages=$PAGES uniq=$UNIQ sorted=$SORTED_OK"

# aws s3 ls --recursive auto-paginates → 2500 lines
N=$(awss3 ls s3://paged --recursive 2>/dev/null | wc -l)
[ "$N" = "2500" ] && ok "aws s3 ls --recursive = 2500 lines" || bad "aws s3 ls --recursive: $N"

# max-keys 5000 → capped to 1000, truncated
J=$(awsapi list-objects-v2 --no-paginate --bucket paged --max-keys 5000)
CNT=$(echo "$J" | python3 -c 'import sys,json;print(len(json.load(sys.stdin).get("Contents",[])))')
TRUNC=$(echo "$J" | python3 -c 'import sys,json;print(str(json.load(sys.stdin).get("IsTruncated")).lower())')
[ "$CNT" = "1000" ] && [ "$TRUNC" = "true" ] && ok "v2 max-keys 5000 capped to 1000, truncated" || bad "v2 cap: cnt=$CNT trunc=$TRUNC"

# start-after k01000 → first key k01001
FIRST=$(awsapi list-objects-v2 --no-paginate --bucket paged --start-after k01000 --max-keys 1 \
  | python3 -c 'import sys,json;print(json.load(sys.stdin)["Contents"][0]["Key"])')
[ "$FIRST" = "k01001" ] && ok "v2 start-after k01000 → k01001" || bad "v2 start-after: $FIRST"

# --- legacy v1 -------------------------------------------------------------
J=$(awsapi list-objects --no-paginate --bucket photos --delimiter /)
HASCP=$(echo "$J" | python3 -c 'import sys,json;print(len(json.load(sys.stdin).get("CommonPrefixes",[])))')
[ "$HASCP" -ge 1 ] && ok "v1 delimiter returns CommonPrefixes" || bad "v1 delimiter CPs: $HASCP"

awsapi create-bucket --bucket listv1nm >/dev/null
seed_rows listv1nm "[f'g{i}/a' for i in range(1,6)]"
NM_WITH=$(awsapi list-objects --no-paginate --bucket listv1nm --delimiter / --max-keys 2 | jget NextMarker)
NM_WITHOUT=$(awsapi list-objects --no-paginate --bucket listv1nm --max-keys 2 | jget NextMarker)
[ -n "$NM_WITH" ] && [ -z "$NM_WITHOUT" ] \
  && ok "v1 NextMarker present with delimiter, absent without" \
  || bad "v1 NextMarker: with='$NM_WITH' without='$NM_WITHOUT'"

# --- encoding-type=url -----------------------------------------------------
printf 'x' > "$WORK/weird"
awsapi put-object --bucket photos --key 'my report (v2).txt' --body "$WORK/weird" >/dev/null
ENCKEY=$(awsapi list-objects-v2 --no-paginate --bucket photos --encoding-type url \
  | python3 -c 'import sys,json;print([o["Key"] for o in json.load(sys.stdin).get("Contents",[]) if "report" in o["Key"]][0])')
[ "$ENCKEY" = "my%20report%20%28v2%29.txt" ] && ok "v2 encoding-type=url encodes weird key" || bad "encoding-type url: $ENCKEY"
rclone lsf r:photos 2>/dev/null | grep -q 'my report (v2).txt' \
  && ok "rclone shows decoded weird key" || bad "rclone decoded key missing"

# --- empty / no-match ------------------------------------------------------
J=$(awsapi list-objects-v2 --no-paginate --bucket photos --prefix nope/)
read -r C KC TR < <(echo "$J" | python3 -c 'import sys,json;d=json.load(sys.stdin);print(len(d.get("Contents",[])), d.get("KeyCount"), str(d.get("IsTruncated")).lower())')
[ "$C" = "0" ] && [ "$KC" = "0" ] && [ "$TR" = "false" ] && ok "v2 no-match prefix empty, KeyCount 0" || bad "v2 no-match: C=$C KC=$KC TR=$TR"

echo "======================================"
echo "PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ]
