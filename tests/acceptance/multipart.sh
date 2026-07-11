#!/usr/bin/env bash
#
# Phase 3 acceptance: drive real boto3 (which auto-switches to multipart at 8MB)
# and the AWS CLI against a live cubby server, exercising the full multipart
# lifecycle and the composite ETag end to end. This is the outer verification
# loop for docs/features/03-multipart-etags-spec.md — the aws-sdk-s3 integration
# tests in tests/s3_api.rs are the inner loop.
#
# Usage:  ./tests/acceptance/multipart.sh
# Requires: aws (v2), python3, cmp, and either boto3 importable or `uv` on PATH
# to bootstrap it into a throwaway venv. Exits non-zero on any failed check.

set -uo pipefail

# --- locate (building if needed) the cubby binary -------------------------
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
BIN="$ROOT/target/debug/cubby"
if [ ! -x "$BIN" ]; then
  echo "building cubby…"
  (cd "$ROOT" && cargo build) || exit 1
fi

for tool in aws python3 cmp; do
  command -v "$tool" >/dev/null || { echo "missing required tool: $tool"; exit 1; }
done

WORK=$(mktemp -d)
DATADIR="$WORK/s3data"
export AWS_ACCESS_KEY_ID=local
export AWS_SECRET_ACCESS_KEY=localsecret
export AWS_DEFAULT_REGION=us-east-1
export AWS_EC2_METADATA_DISABLED=true

# --- ensure boto3 is importable (bootstrap via uv if needed) ---------------
PY=python3
if ! python3 -c 'import boto3' 2>/dev/null; then
  if command -v uv >/dev/null; then
    echo "bootstrapping boto3 into a throwaway venv via uv…"
    uv venv "$WORK/venv" >/dev/null 2>&1 || { echo "uv venv failed"; exit 1; }
    VIRTUAL_ENV="$WORK/venv" uv pip install --python "$WORK/venv/bin/python" boto3 >/dev/null 2>&1 \
      || { echo "uv pip install boto3 failed"; exit 1; }
    PY="$WORK/venv/bin/python"
  else
    echo "boto3 not importable and uv not found; cannot run acceptance"; exit 1
  fi
fi

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
export CUBBY_EP="$EP"
echo "server at $EP"

awss3()  { aws --endpoint-url "$EP" s3 "$@"; }
awsapi() { aws --endpoint-url "$EP" s3api "$@"; }

awsapi create-bucket --bucket bkt >/dev/null

echo "=============== boto3 ==============="

# --- 1) boto3 100MB round-trip (the headline) ------------------------------
head -c 104857600 /dev/urandom > "$WORK/big.bin"
"$PY" - "$WORK/big.bin" <<'PY'
import sys, hashlib, boto3, os
from boto3.s3.transfer import TransferConfig
ep = os.environ["CUBBY_EP"]
src = sys.argv[1]
s3 = boto3.client("s3", endpoint_url=ep, aws_access_key_id="local",
                  aws_secret_access_key="localsecret", region_name="us-east-1")
# Default TransferConfig auto-multiparts above 8MB.
s3.upload_file(src, "bkt", "big.bin")
dst = src + ".dl"
s3.download_file("bkt", "big.bin", dst)
def md5(p):
    h = hashlib.md5()
    with open(p, "rb") as f:
        for c in iter(lambda: f.read(1 << 20), b""):
            h.update(c)
    return h.hexdigest()
assert md5(src) == md5(dst), "round-trip md5 mismatch"
print("OK")
PY
if [ $? -eq 0 ]; then ok "boto3 100MB upload_file/download_file md5 round-trip"; else bad "boto3 100MB round-trip"; fi

# On-disk file cmps clean against the source (one real assembled file).
if cmp -s "$WORK/big.bin" "$DATADIR/buckets/bkt/big.bin"; then
  ok "cmp s3data/buckets/bkt/big.bin against source is clean"
else
  bad "on-disk assembled file does not cmp against source"
fi

# --- 2) composite ETag matches the formula ---------------------------------
ETAG=$(awsapi head-object --bucket bkt --key big.bin --query ETag --output text | tr -d '"')
echo "$ETAG" | grep -qE '^[0-9a-f]{32}-[0-9]+$' \
  && ok "head-object ETag has <32hex>-N form ($ETAG)" \
  || bad "ETag not composite form: $ETAG"

# Independently recompute md5-of-md5s-N over 8MB chunks (boto3's default part
# size) and compare.
"$PY" - "$WORK/big.bin" "$ETAG" <<'PY'
import sys, hashlib
src, etag = sys.argv[1], sys.argv[2]
part = 8 * 1024 * 1024  # boto3 TransferConfig default multipart_chunksize
digests = []
with open(src, "rb") as f:
    while True:
        b = f.read(part)
        if not b:
            break
        digests.append(hashlib.md5(b).digest())
comp = hashlib.md5(b"".join(digests)).hexdigest() + f"-{len(digests)}"
assert comp == etag, f"computed {comp} != returned {etag}"
print("OK")
PY
if [ $? -eq 0 ]; then ok "composite ETag equals independent md5-of-md5s-N"; else bad "composite ETag mismatch"; fi

# --- 3) explicit low-level lifecycle (boto3) -------------------------------
# CUBBY_DATADIR is exported so the staging-dir filesystem assertion can see it.
CUBBY_DATADIR="$DATADIR" "$PY" - <<'PY'
import os, boto3
ep = os.environ["CUBBY_EP"]
s3 = boto3.client("s3", endpoint_url=ep, aws_access_key_id="local",
                  aws_secret_access_key="localsecret", region_name="us-east-1")
key = "lifecycle.bin"
mpu = s3.create_multipart_upload(Bucket="bkt", Key=key)
uid = mpu["UploadId"]
parts, bodies = [], [b"A"*1000, b"B"*2000, b"C"*3000]
for i, body in enumerate(bodies, start=1):
    r = s3.upload_part(Bucket="bkt", Key=key, UploadId=uid, PartNumber=i, Body=body)
    parts.append({"PartNumber": i, "ETag": r["ETag"]})
out = s3.complete_multipart_upload(Bucket="bkt", Key=key, UploadId=uid,
                                   MultipartUpload={"Parts": parts})
assert out["ETag"].strip('"').endswith("-3"), out["ETag"]
got = s3.get_object(Bucket="bkt", Key=key)["Body"].read()
assert got == b"".join(bodies)
staging = os.path.join(os.environ["CUBBY_DATADIR"], ".multipart", uid)
assert not os.path.exists(staging), f"staging {staging} still exists"
print("OK")
PY
if [ $? -eq 0 ]; then ok "explicit create/upload_part×3/complete; bytes concat; staging gone"; else bad "low-level lifecycle"; fi

# --- 8) overwrite (boto3): single-PUT then multipart to the same key -------
CUBBY_DATADIR="$DATADIR" "$PY" - <<'PY'
import os, boto3
ep = os.environ["CUBBY_EP"]
s3 = boto3.client("s3", endpoint_url=ep, aws_access_key_id="local",
                  aws_secret_access_key="localsecret", region_name="us-east-1")
key = "over.bin"
s3.put_object(Bucket="bkt", Key=key, Body=b"original-single-put")
mpu = s3.create_multipart_upload(Bucket="bkt", Key=key)
uid = mpu["UploadId"]
bodies = [b"x"*3000, b"y"*4000]
parts = []
for i, body in enumerate(bodies, start=1):
    r = s3.upload_part(Bucket="bkt", Key=key, UploadId=uid, PartNumber=i, Body=body)
    parts.append({"PartNumber": i, "ETag": r["ETag"]})
out = s3.complete_multipart_upload(Bucket="bkt", Key=key, UploadId=uid,
                                   MultipartUpload={"Parts": parts})
assert out["ETag"].strip('"').endswith("-2"), out["ETag"]
got = s3.get_object(Bucket="bkt", Key=key)["Body"].read()
assert got == b"".join(bodies), "multipart content did not replace single-put"
disk = os.path.join(os.environ["CUBBY_DATADIR"], "buckets", "bkt", key)
assert open(disk, "rb").read() == b"".join(bodies), "on-disk file not overwritten"
print("OK")
PY
if [ $? -eq 0 ]; then ok "overwrite: multipart replaces single-PUT (last writer wins)"; else bad "overwrite"; fi

# --- 9) Range GET on assembled object (boto3) ------------------------------
"$PY" - <<'PY'
import os, boto3
ep = os.environ["CUBBY_EP"]
s3 = boto3.client("s3", endpoint_url=ep, aws_access_key_id="local",
                  aws_secret_access_key="localsecret", region_name="us-east-1")
r = s3.get_object(Bucket="bkt", Key="big.bin", Range="bytes=8388600-8388700")
data = r["Body"].read()
assert len(data) == 101, f"expected 101 bytes, got {len(data)}"
print("OK")
PY
if [ $? -eq 0 ]; then ok "Range GET bytes=8388600-8388700 returns exactly 101 bytes"; else bad "Range GET on assembled object"; fi

echo "=============== aws cli ==============="

# --- 4) AWS CLI large upload (auto-multipart > 8MB) ------------------------
if awss3 cp "$WORK/big.bin" s3://bkt/cli.bin >/dev/null 2>"$WORK/cp.log"; then
  awss3 cp s3://bkt/cli.bin "$WORK/cli.dl" >/dev/null 2>&1
  if cmp -s "$WORK/big.bin" "$WORK/cli.dl"; then ok "aws s3 cp large upload round-trips"; else bad "aws s3 cp round-trip mismatch"; fi
  CLIETAG=$(awsapi head-object --bucket bkt --key cli.bin --query ETag --output text | tr -d '"')
  echo "$CLIETAG" | grep -qE '^[0-9a-f]{32}-[0-9]+$' && ok "aws head-object shows -N ETag ($CLIETAG)" || bad "cli ETag not composite: $CLIETAG"
else
  bad "aws s3 cp large upload exit nonzero"; cat "$WORK/cp.log"
fi

# --- 5) ListParts (AWS CLI): upload parts 1 & 2, do not complete -----------
UID2=$(awsapi create-multipart-upload --bucket bkt --key parts.bin --query UploadId --output text)
printf 'AAAAAAAAAA' > "$WORK/p1"       # 10 bytes
printf 'BBBBBBBBBBBBBBBBBBBB' > "$WORK/p2"  # 20 bytes
awsapi upload-part --bucket bkt --key parts.bin --upload-id "$UID2" --part-number 1 --body "$WORK/p1" >/dev/null
awsapi upload-part --bucket bkt --key parts.bin --upload-id "$UID2" --part-number 2 --body "$WORK/p2" >/dev/null
LP=$(awsapi list-parts --bucket bkt --key parts.bin --upload-id "$UID2")
NUMS=$(echo "$LP" | "$PY" -c 'import sys,json;print(",".join(str(p["PartNumber"]) for p in json.load(sys.stdin)["Parts"]))')
SIZES=$(echo "$LP" | "$PY" -c 'import sys,json;print(",".join(str(p["Size"]) for p in json.load(sys.stdin)["Parts"]))')
[ "$NUMS" = "1,2" ] && [ "$SIZES" = "10,20" ] \
  && ok "list-parts shows parts 1,2 ascending with sizes 10,20" \
  || bad "list-parts: nums=$NUMS sizes=$SIZES"

# --- 6) Abort (AWS CLI) ----------------------------------------------------
awsapi abort-multipart-upload --bucket bkt --key parts.bin --upload-id "$UID2" >/dev/null
[ ! -d "$DATADIR/.multipart/$UID2" ] && ok "abort removed staging dir" || bad "staging dir survived abort"
if awsapi list-parts --bucket bkt --key parts.bin --upload-id "$UID2" >/dev/null 2>"$WORK/lp.err"; then
  bad "list-parts after abort should fail"
else
  grep -q NoSuchUpload "$WORK/lp.err" && ok "list-parts after abort → NoSuchUpload" || { bad "list-parts after abort wrong error"; cat "$WORK/lp.err"; }
fi
if awsapi head-object --bucket bkt --key parts.bin >/dev/null 2>&1; then
  bad "head-object after abort should 404 (no object created)"
else
  ok "head-object after abort → 404 (no object created)"
fi

# --- 7) Complete error paths ----------------------------------------------
UID3=$(awsapi create-multipart-upload --bucket bkt --key err.bin --query UploadId --output text)
E1=$(awsapi upload-part --bucket bkt --key err.bin --upload-id "$UID3" --part-number 1 --body "$WORK/p1" --query ETag --output text)
E2=$(awsapi upload-part --bucket bkt --key err.bin --upload-id "$UID3" --part-number 2 --body "$WORK/p2" --query ETag --output text)

# (a) wrong part ETag → InvalidPart
if awsapi complete-multipart-upload --bucket bkt --key err.bin --upload-id "$UID3" \
   --multipart-upload 'Parts=[{PartNumber=1,ETag="00000000000000000000000000000000"}]' \
   >/dev/null 2>"$WORK/e.err"; then bad "wrong ETag should fail"; else
  grep -q InvalidPart "$WORK/e.err" && ok "(a) wrong part ETag → InvalidPart" || { bad "(a) wrong code"; cat "$WORK/e.err"; }
fi
# (b) descending order → InvalidPartOrder
if awsapi complete-multipart-upload --bucket bkt --key err.bin --upload-id "$UID3" \
   --multipart-upload "Parts=[{PartNumber=2,ETag=$E2},{PartNumber=1,ETag=$E1}]" \
   >/dev/null 2>"$WORK/e.err"; then bad "descending order should fail"; else
  grep -q InvalidPartOrder "$WORK/e.err" && ok "(b) descending order → InvalidPartOrder" || { bad "(b) wrong code"; cat "$WORK/e.err"; }
fi
# (c) bogus upload id on complete/list/upload → NoSuchUpload
BOGUS=deadbeefdeadbeefdeadbeefdeadbeef
if awsapi list-parts --bucket bkt --key err.bin --upload-id "$BOGUS" >/dev/null 2>"$WORK/e.err"; then
  bad "bogus upload id list should fail"; else
  grep -q NoSuchUpload "$WORK/e.err" && ok "(c) bogus upload-id → NoSuchUpload" || { bad "(c) wrong code"; cat "$WORK/e.err"; }
fi
# (d) create on absent bucket → NoSuchBucket
if awsapi create-multipart-upload --bucket ghostbucket --key k >/dev/null 2>"$WORK/e.err"; then
  bad "create on absent bucket should fail"; else
  grep -q NoSuchBucket "$WORK/e.err" && ok "(d) create on absent bucket → NoSuchBucket" || { bad "(d) wrong code"; cat "$WORK/e.err"; }
fi

# --- 10) listing shows the composite ETag ----------------------------------
LOETAG=$(awsapi list-objects-v2 --no-paginate --bucket bkt \
  | "$PY" -c 'import sys,json;print([o["ETag"] for o in json.load(sys.stdin)["Contents"] if o["Key"]=="big.bin"][0].strip("\""))')
echo "$LOETAG" | grep -qE '^[0-9a-f]{32}-[0-9]+$' \
  && ok "list-objects-v2 carries big.bin composite ETag ($LOETAG)" \
  || bad "list-objects-v2 ETag not composite: $LOETAG"

echo "======================================"
echo "PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ]
