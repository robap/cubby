#!/usr/bin/env python3
"""boto3 conformance: >8MB multipart round-trip, prefix+delimiter listing, and a
credential-less presigned GET against a live cubby (path-style, plain HTTP)."""

import hashlib
import os
import sys
import urllib.request

import boto3
from botocore.config import Config

EP = os.environ["CUBBY_EP"]
BUCKET = os.environ["CUBBY_BUCKET"]
BIG = os.environ["CUBBY_BIG"]
WORK = os.environ["CUBBY_WORK"]

s3 = boto3.client(
    "s3",
    endpoint_url=EP,
    aws_access_key_id="local",
    aws_secret_access_key="localsecret",
    region_name="us-east-1",
    # cubby is path-style only (no virtual-host bucket.localhost).
    config=Config(s3={"addressing_style": "path"}),
)

fails = 0


def check(name, cond):
    global fails
    print(("ok  : " if cond else "FAIL: ") + name)
    if not cond:
        fails += 1


def md5(path):
    h = hashlib.md5()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


s3.create_bucket(Bucket=BUCKET)

# 1) round-trip incl. a >8MB upload (boto3's default TransferConfig auto-switches
#    to multipart above 8MB), bytes verified equal.
s3.upload_file(BIG, BUCKET, "big.bin")
dst = os.path.join(WORK, "boto3.dl")
s3.download_file(BUCKET, "big.bin", dst)
check("multipart >8MB round-trip: downloaded bytes equal", md5(BIG) == md5(dst))
etag = s3.head_object(Bucket=BUCKET, Key="big.bin")["ETag"].strip('"')
check(f"multipart ETag is composite (-N): {etag}", "-" in etag)

# 2) list a nested layout with prefix + `/` delimiter.
for key, body in [
    ("docs/a.txt", b"a"),
    ("docs/b.txt", b"b"),
    ("docs/img/c.txt", b"c"),
    ("top.txt", b"t"),
]:
    s3.put_object(Bucket=BUCKET, Key=key, Body=body)
resp = s3.list_objects_v2(Bucket=BUCKET, Prefix="docs/", Delimiter="/")
keys = [o["Key"] for o in resp.get("Contents", [])]
cps = [p["Prefix"] for p in resp.get("CommonPrefixes", [])]
check(f"delimiter list keys == [docs/a.txt, docs/b.txt]: {keys}",
      keys == ["docs/a.txt", "docs/b.txt"])
check(f"delimiter list CommonPrefixes == [docs/img/]: {cps}", cps == ["docs/img/"])

# 3) presigned GET fetched with NO ambient credentials (query-string auth).
s3.put_object(Bucket=BUCKET, Key="signed.txt", Body=b"presigned-bytes")
url = s3.generate_presigned_url(
    "get_object", Params={"Bucket": BUCKET, "Key": "signed.txt"}, ExpiresIn=300
)
with urllib.request.urlopen(url) as r:  # noqa: S310 (local http, no creds)
    body = r.read()
    code = r.getcode()
check(f"presigned GET returns 200 + bytes (status {code})",
      code == 200 and body == b"presigned-bytes")

sys.exit(1 if fails else 0)
