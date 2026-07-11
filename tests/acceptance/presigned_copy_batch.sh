#!/usr/bin/env bash
#
# Phase 4 acceptance: drive real boto3, aws-sdk-js v3, and the AWS CLI against a
# live buckit server, proving presigned (query-string) auth, CopyObject, and
# batch DeleteObjects end to end. This is the outer verification loop for
# docs/features/04-presigned-copy-batch-spec.md — the aws-sdk-s3 integration
# tests in tests/s3_api.rs are the inner loop.
#
# Usage:  ./tests/acceptance/presigned_copy_batch.sh
# Requires: aws (v2), python3, curl, cmp, and either boto3 importable or `uv` on
# PATH. aws-sdk-js v3 checks additionally need node + npm (skipped with a warning
# if absent or offline). Exits non-zero on any failed check.

set -uo pipefail

# --- locate (building if needed) the buckit binary -------------------------
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
BIN="$ROOT/target/debug/buckit"
if [ ! -x "$BIN" ]; then
  echo "building buckit…"
  (cd "$ROOT" && cargo build) || exit 1
fi

for tool in aws python3 curl cmp; do
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
export BUCKIT_EP="$EP"
echo "server at $EP"

awss3()  { aws --endpoint-url "$EP" s3 "$@"; }
awsapi() { aws --endpoint-url "$EP" s3api "$@"; }

awsapi create-bucket --bucket bkt >/dev/null
awsapi create-bucket --bucket bkt2 >/dev/null

echo "=============== presigned URLs ==============="

echo "hello presigned world" > "$WORK/hello.txt"
awsapi put-object --bucket bkt --key hello.txt --body "$WORK/hello.txt" >/dev/null

# --- 1) boto3 presigned GET (the headline) ---------------------------------
"$PY" - <<'PY'
import os, urllib.request, boto3
ep = os.environ["BUCKIT_EP"]
s3 = boto3.client("s3", endpoint_url=ep, aws_access_key_id="local",
                  aws_secret_access_key="localsecret", region_name="us-east-1")
url = s3.generate_presigned_url("get_object", Params={"Bucket": "bkt", "Key": "hello.txt"})
# Plain HTTP client, no AWS credentials anywhere.
with urllib.request.urlopen(url) as r:
    assert r.status == 200, r.status
    body = r.read()
assert body == b"hello presigned world\n", body
print("OK")
PY
[ $? -eq 0 ] && ok "boto3 presigned GET (no creds) → 200 + exact bytes" || bad "boto3 presigned GET"

# --- 2) boto3 presigned PUT ------------------------------------------------
# boto3 defaults `generate_presigned_url("put_object")` to the legacy SigV2
# query scheme; S3-compatible endpoints (buckit, MinIO, …) want SigV4, so the
# client pins `signature_version="s3v4"`. GET happens to default to SigV4.
BUCKIT_DATADIR="$DATADIR" "$PY" - <<'PY'
import os, urllib.request, boto3
from botocore.config import Config
ep = os.environ["BUCKIT_EP"]
s3 = boto3.client("s3", endpoint_url=ep, aws_access_key_id="local",
                  aws_secret_access_key="localsecret", region_name="us-east-1",
                  config=Config(signature_version="s3v4"))
url = s3.generate_presigned_url("put_object", Params={"Bucket": "bkt", "Key": "up.txt"})
assert "X-Amz-Signature" in url, "expected a SigV4 presigned URL"
payload = b"uploaded via UNSIGNED-PAYLOAD presigned PUT"
req = urllib.request.Request(url, data=payload, method="PUT")
with urllib.request.urlopen(req) as r:
    assert r.status == 200, r.status
# Authed GET returns the bytes.
got = s3.get_object(Bucket="bkt", Key="up.txt")["Body"].read()
assert got == payload, got
# Real file on disk, cmp-clean.
disk = os.path.join(os.environ["BUCKIT_DATADIR"], "buckets", "bkt", "up.txt")
assert open(disk, "rb").read() == payload, "on-disk bytes differ"
print("OK")
PY
[ $? -eq 0 ] && ok "boto3 presigned PUT (no creds) → 200; authed GET + cmp clean" || bad "boto3 presigned PUT"

# --- 3) AWS CLI presign + curl ---------------------------------------------
URL=$(awss3 presign s3://bkt/hello.txt)
CODE=$(curl -s -o "$WORK/cli_get.txt" -w '%{http_code}' "$URL")
if [ "$CODE" = "200" ] && cmp -s "$WORK/hello.txt" "$WORK/cli_get.txt"; then
  ok "aws s3 presign + curl → 200 + bytes"
else
  bad "aws s3 presign (http=$CODE)"
fi

# --- 4) expired presigned URL → 403 AccessDenied ---------------------------
EURL=$(awss3 presign s3://bkt/hello.txt --expires-in 1)
sleep 2
curl -s -o "$WORK/exp.txt" "$EURL"
if grep -q 'AccessDenied' "$WORK/exp.txt"; then
  ok "expired presigned URL → 403 AccessDenied"
else
  bad "expired presigned URL wrong response"; cat "$WORK/exp.txt"
fi

# --- 5) tampered presigned URL → 403 SignatureDoesNotMatch -----------------
TURL=$(echo "$URL" | sed 's#/hello.txt#/hello-tampered.txt#')
curl -s -o "$WORK/tam.txt" "$TURL"
if grep -q 'SignatureDoesNotMatch' "$WORK/tam.txt"; then
  ok "tampered presigned URL → 403 SignatureDoesNotMatch"
else
  bad "tampered presigned URL wrong response"; cat "$WORK/tam.txt"
fi

# --- 6) aws-sdk-js v3 presigned GET (best-effort) --------------------------
if command -v node >/dev/null && command -v npm >/dev/null; then
  JSDIR="$WORK/jsp"
  mkdir -p "$JSDIR"
  ( cd "$JSDIR" && npm init -y >/dev/null 2>&1 && \
    npm install @aws-sdk/client-s3 @aws-sdk/s3-request-presigner >/dev/null 2>&1 )
  if [ -d "$JSDIR/node_modules/@aws-sdk/s3-request-presigner" ]; then
    cat > "$JSDIR/presign.mjs" <<'JS'
import { S3Client, GetObjectCommand } from "@aws-sdk/client-s3";
import { getSignedUrl } from "@aws-sdk/s3-request-presigner";
const ep = process.env.BUCKIT_EP;
const s3 = new S3Client({
  endpoint: ep, region: "us-east-1", forcePathStyle: true,
  credentials: { accessKeyId: "local", secretAccessKey: "localsecret" },
});
const url = await getSignedUrl(s3, new GetObjectCommand({ Bucket: "bkt", Key: "hello.txt" }), { expiresIn: 3600 });
const res = await fetch(url); // no credentials
const text = await res.text();
if (res.status !== 200) { console.error("status", res.status, text); process.exit(1); }
if (text !== "hello presigned world\n") { console.error("body mismatch:", JSON.stringify(text)); process.exit(1); }
console.log("OK");
JS
    if ( cd "$JSDIR" && BUCKIT_EP="$EP" node presign.mjs ); then
      ok "aws-sdk-js v3 getSignedUrl + fetch (no creds) → 200 + bytes"
    else
      bad "aws-sdk-js v3 presigned GET"
    fi
  else
    echo "SKIP: aws-sdk-js v3 (npm install failed / offline)"
  fi
else
  echo "SKIP: aws-sdk-js v3 (node/npm not available)"
fi

echo "=============== CopyObject ==============="

# --- 7) boto3 copy: bytes + preserved source ETag --------------------------
head -c 4096 /dev/urandom > "$WORK/src.bin"
awsapi put-object --bucket bkt --key src.bin --body "$WORK/src.bin" >/dev/null
BUCKIT_DATADIR="$DATADIR" "$PY" - <<'PY'
import os, boto3
ep = os.environ["BUCKIT_EP"]
s3 = boto3.client("s3", endpoint_url=ep, aws_access_key_id="local",
                  aws_secret_access_key="localsecret", region_name="us-east-1")
src_etag = s3.head_object(Bucket="bkt", Key="src.bin")["ETag"]
s3.copy_object(CopySource="bkt/src.bin", Bucket="bkt", Key="dst.bin")
dst = s3.head_object(Bucket="bkt", Key="dst.bin")
assert dst["ETag"] == src_etag, (dst["ETag"], src_etag)
body = s3.get_object(Bucket="bkt", Key="dst.bin")["Body"].read()
assert body == open(os.path.join(os.environ["BUCKIT_DATADIR"], "buckets", "bkt", "src.bin"), "rb").read()
print("OK")
PY
[ $? -eq 0 ] && ok "boto3 copy_object: dst bytes == src, dest ETag == source ETag" || bad "boto3 copy_object"
cmp -s "$DATADIR/buckets/bkt/dst.bin" "$DATADIR/buckets/bkt/src.bin" \
  && ok "cmp s3data/buckets/bkt/dst.bin src.bin clean (real file)" \
  || bad "copied file does not cmp against source"

# --- 8) AWS CLI cross-bucket copy ------------------------------------------
if awss3 cp s3://bkt/src.bin s3://bkt2/copied.bin >/dev/null 2>"$WORK/cp.err"; then
  if [ -f "$DATADIR/buckets/bkt2/copied.bin" ] && cmp -s "$DATADIR/buckets/bkt2/copied.bin" "$WORK/src.bin"; then
    ok "aws s3 cp cross-bucket: bkt2/copied.bin exists with source bytes"
  else
    bad "cross-bucket copy: dest file missing or mismatched"
  fi
else
  bad "aws s3 cp cross-bucket exit nonzero"; cat "$WORK/cp.err"
fi

# --- 9) metadata-directive COPY carries source metadata --------------------
"$PY" - <<'PY'
import os, boto3
ep = os.environ["BUCKIT_EP"]
s3 = boto3.client("s3", endpoint_url=ep, aws_access_key_id="local",
                  aws_secret_access_key="localsecret", region_name="us-east-1")
s3.put_object(Bucket="bkt", Key="meta-src", Body=b"{}",
              ContentType="application/json", Metadata={"team": "x"})
s3.copy_object(CopySource="bkt/meta-src", Bucket="bkt", Key="meta-dst")  # default COPY
h = s3.head_object(Bucket="bkt", Key="meta-dst")
assert h["ContentType"] == "application/json", h["ContentType"]
assert h["Metadata"].get("team") == "x", h["Metadata"]
print("OK")
PY
[ $? -eq 0 ] && ok "copy COPY-directive carries source content-type + metadata" || bad "copy COPY metadata"

# --- 10) source==dest REPLACE (metadata-only update) -----------------------
BUCKIT_DATADIR="$DATADIR" "$PY" - <<'PY'
import os, boto3
ep = os.environ["BUCKIT_EP"]
s3 = boto3.client("s3", endpoint_url=ep, aws_access_key_id="local",
                  aws_secret_access_key="localsecret", region_name="us-east-1")
s3.put_object(Bucket="bkt", Key="k", Body=b"original-bytes", ContentType="application/octet-stream")
before = open(os.path.join(os.environ["BUCKIT_DATADIR"], "buckets", "bkt", "k"), "rb").read()
s3.copy_object(CopySource="bkt/k", Bucket="bkt", Key="k",
               MetadataDirective="REPLACE", ContentType="text/plain", Metadata={"v": "2"})
h = s3.head_object(Bucket="bkt", Key="k")
assert h["ContentType"] == "text/plain", h["ContentType"]
assert h["Metadata"].get("v") == "2", h["Metadata"]
after = open(os.path.join(os.environ["BUCKIT_DATADIR"], "buckets", "bkt", "k"), "rb").read()
assert before == after == b"original-bytes", "bytes must be untouched"
print("OK")
PY
[ $? -eq 0 ] && ok "source==dest REPLACE: metadata updated, bytes untouched" || bad "source==dest REPLACE"

# --- 11) source==dest COPY → InvalidRequest --------------------------------
"$PY" - <<'PY'
import os, boto3
from botocore.exceptions import ClientError
ep = os.environ["BUCKIT_EP"]
s3 = boto3.client("s3", endpoint_url=ep, aws_access_key_id="local",
                  aws_secret_access_key="localsecret", region_name="us-east-1")
try:
    s3.copy_object(CopySource="bkt/k", Bucket="bkt", Key="k")  # default COPY
except ClientError as e:
    assert e.response["Error"]["Code"] == "InvalidRequest", e.response["Error"]["Code"]
    print("OK"); raise SystemExit(0)
raise SystemExit("self-copy with COPY should have failed")
PY
[ $? -eq 0 ] && ok "source==dest COPY → InvalidRequest" || bad "source==dest COPY"

# --- 12) copy errors: source key / bucket missing --------------------------
"$PY" - <<'PY'
import os, boto3
from botocore.exceptions import ClientError
ep = os.environ["BUCKIT_EP"]
s3 = boto3.client("s3", endpoint_url=ep, aws_access_key_id="local",
                  aws_secret_access_key="localsecret", region_name="us-east-1")
def code(cs):
    try:
        s3.copy_object(CopySource=cs, Bucket="bkt", Key="x")
    except ClientError as e:
        return e.response["Error"]["Code"]
    return "NO-ERROR"
assert code("bkt/does-not-exist") == "NoSuchKey", code("bkt/does-not-exist")
assert code("no-bucket/k") == "NoSuchBucket", code("no-bucket/k")
print("OK")
PY
[ $? -eq 0 ] && ok "copy source missing → NoSuchKey; source bucket missing → NoSuchBucket" || bad "copy error codes"

# --- 13) copy of a multipart object preserves the composite ETag -----------
head -c 20971520 /dev/urandom > "$WORK/mp.bin"   # 20MB → boto3 multiparts >8MB
"$PY" - "$WORK/mp.bin" <<'PY'
import os, sys, boto3
ep = os.environ["BUCKIT_EP"]
s3 = boto3.client("s3", endpoint_url=ep, aws_access_key_id="local",
                  aws_secret_access_key="localsecret", region_name="us-east-1")
s3.upload_file(sys.argv[1], "bkt", "mp.bin")  # auto-multipart
src_etag = s3.head_object(Bucket="bkt", Key="mp.bin")["ETag"]
assert src_etag.strip('"').count("-") == 1 and src_etag.strip('"').split("-")[1].isdigit(), src_etag
s3.copy_object(CopySource="bkt/mp.bin", Bucket="bkt", Key="mp-copy.bin")
dst_etag = s3.head_object(Bucket="bkt", Key="mp-copy.bin")["ETag"]
assert dst_etag == src_etag, (dst_etag, src_etag)
print("OK")
PY
[ $? -eq 0 ] && ok "copy of multipart object preserves composite -N ETag" || bad "multipart copy ETag"
cmp -s "$DATADIR/buckets/bkt/mp-copy.bin" "$WORK/mp.bin" \
  && ok "multipart copy on-disk file cmp clean" || bad "multipart copy bytes differ"

echo "=============== DeleteObjects (batch) ==============="

# --- 14) boto3 batch delete ------------------------------------------------
BUCKIT_DATADIR="$DATADIR" "$PY" - <<'PY'
import os, boto3
ep = os.environ["BUCKIT_EP"]
s3 = boto3.client("s3", endpoint_url=ep, aws_access_key_id="local",
                  aws_secret_access_key="localsecret", region_name="us-east-1")
for k in ("d1", "d2", "d3"):
    s3.put_object(Bucket="bkt", Key=k, Body=b"x")
resp = s3.delete_objects(Bucket="bkt", Delete={"Objects": [{"Key": "d1"}, {"Key": "d2"}, {"Key": "d3"}]})
deleted = sorted(d["Key"] for d in resp.get("Deleted", []))
assert deleted == ["d1", "d2", "d3"], deleted
assert not resp.get("Errors"), resp.get("Errors")
base = os.path.join(os.environ["BUCKIT_DATADIR"], "buckets", "bkt")
for k in ("d1", "d2", "d3"):
    assert not os.path.exists(os.path.join(base, k)), f"{k} still on disk"
present = {o["Key"] for o in s3.list_objects_v2(Bucket="bkt").get("Contents", [])}
assert not ({"d1", "d2", "d3"} & present), present
print("OK")
PY
[ $? -eq 0 ] && ok "boto3 batch delete: all listed, files gone, list clean" || bad "boto3 batch delete"

# --- 15) batch delete is idempotent (never-existed key) --------------------
"$PY" - <<'PY'
import os, boto3
ep = os.environ["BUCKIT_EP"]
s3 = boto3.client("s3", endpoint_url=ep, aws_access_key_id="local",
                  aws_secret_access_key="localsecret", region_name="us-east-1")
s3.put_object(Bucket="bkt", Key="real1", Body=b"x")
resp = s3.delete_objects(Bucket="bkt", Delete={"Objects": [{"Key": "real1"}, {"Key": "ghost-key"}]})
deleted = sorted(d["Key"] for d in resp.get("Deleted", []))
assert deleted == ["ghost-key", "real1"], deleted
assert not resp.get("Errors"), resp.get("Errors")
print("OK")
PY
[ $? -eq 0 ] && ok "batch delete idempotent: never-existed key reported deleted" || bad "batch delete idempotency"

# --- 16) quiet mode --------------------------------------------------------
BUCKIT_DATADIR="$DATADIR" "$PY" - <<'PY'
import os, boto3
ep = os.environ["BUCKIT_EP"]
s3 = boto3.client("s3", endpoint_url=ep, aws_access_key_id="local",
                  aws_secret_access_key="localsecret", region_name="us-east-1")
for k in ("q1", "q2"):
    s3.put_object(Bucket="bkt", Key=k, Body=b"x")
resp = s3.delete_objects(Bucket="bkt", Delete={"Objects": [{"Key": "q1"}, {"Key": "q2"}], "Quiet": True})
assert not resp.get("Deleted"), resp.get("Deleted")
assert not resp.get("Errors"), resp.get("Errors")
base = os.path.join(os.environ["BUCKIT_DATADIR"], "buckets", "bkt")
for k in ("q1", "q2"):
    assert not os.path.exists(os.path.join(base, k)), f"{k} still on disk"
print("OK")
PY
[ $? -eq 0 ] && ok "quiet mode: no Deleted/Errors, keys removed from disk" || bad "quiet mode"

# --- 17) aws s3 rm --recursive (batch under the hood) ----------------------
for n in a b c; do awsapi put-object --bucket bkt --key "pre/$n" --body "$WORK/hello.txt" >/dev/null; done
awss3 rm s3://bkt/pre/ --recursive >/dev/null 2>"$WORK/rm.err"
RC=$?
LEFT=$(ls "$DATADIR/buckets/bkt/pre" 2>/dev/null | wc -l)
if [ "$RC" -eq 0 ] && [ "$LEFT" -eq 0 ]; then
  ok "aws s3 rm --recursive removed all keys under prefix on disk"
else
  bad "aws s3 rm --recursive (rc=$RC left=$LEFT)"; cat "$WORK/rm.err"
fi

echo "======================================"
echo "PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ]
