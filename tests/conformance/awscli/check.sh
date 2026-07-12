#!/usr/bin/env bash
#
# AWS CLI conformance: `aws s3 cp` >8MB round-trip (auto-multipart), a
# prefix+`/` delimiter listing, and an `aws s3 presign` URL fetched by curl with
# no ambient credentials.

set -uo pipefail
EP="$CUBBY_EP"; B="$CUBBY_BUCKET"; BIG="$CUBBY_BIG"; W="$CUBBY_WORK"

fails=0
check() { if [ "$2" = 1 ]; then echo "ok  : $1"; else echo "FAIL: $1"; fails=$((fails + 1)); fi; }
awss3()  { aws --endpoint-url "$EP" s3 "$@"; }
awsapi() { aws --endpoint-url "$EP" s3api "$@"; }

awsapi create-bucket --bucket "$B" >/dev/null

# 1) >8MB round-trip (CLI auto-multiparts above its 8MB threshold).
awss3 cp "$BIG" "s3://$B/big.bin" >/dev/null 2>&1
awss3 cp "s3://$B/big.bin" "$W/awscli.dl" >/dev/null 2>&1
cmp -s "$BIG" "$W/awscli.dl" \
  && check "multipart >8MB round-trip: bytes equal" 1 \
  || check "multipart >8MB round-trip: bytes equal" 0
ETAG=$(awsapi head-object --bucket "$B" --key big.bin --query ETag --output text | tr -d '"')
case "$ETAG" in
  *-*) check "multipart ETag is composite (-N): $ETAG" 1 ;;
  *)   check "multipart ETag is composite (-N): $ETAG" 0 ;;
esac

# 2) prefix + `/` delimiter listing. Assert the structured result via s3api…
printf a > "$W/a"; printf b > "$W/b"; printf c > "$W/c"; printf t > "$W/t"
awss3 cp "$W/a" "s3://$B/docs/a.txt"     >/dev/null
awss3 cp "$W/b" "s3://$B/docs/b.txt"     >/dev/null
awss3 cp "$W/c" "s3://$B/docs/img/c.txt" >/dev/null
awss3 cp "$W/t" "s3://$B/top.txt"        >/dev/null
LS=$(awsapi list-objects-v2 --bucket "$B" --prefix docs/ --delimiter /)
KEYS=$(echo "$LS" | jq -r '[.Contents[].Key] | join(",")')
CPS=$(echo "$LS" | jq -r '[.CommonPrefixes[].Prefix] | join(",")')
[ "$KEYS" = "docs/a.txt,docs/b.txt" ] \
  && check "delimiter list keys == docs/a.txt,docs/b.txt" 1 \
  || check "delimiter list keys ($KEYS)" 0
[ "$CPS" = "docs/img/" ] \
  && check "delimiter list CommonPrefixes == docs/img/" 1 \
  || check "delimiter list CommonPrefixes ($CPS)" 0
# …and prove the `aws s3 ls s3://b/prefix/` folder view shows the sub-prefix.
# Capture first (piping `aws` into `grep -q` closes the pipe early → SIGPIPE).
LS_FOLDER=$(awss3 ls "s3://$B/docs/")
case "$LS_FOLDER" in
  *"PRE img/"*) check "aws s3 ls s3://$B/docs/ shows 'PRE img/'" 1 ;;
  *)            check "aws s3 ls s3://$B/docs/ shows 'PRE img/'" 0 ;;
esac

# 3) presigned GET fetched credential-less.
printf 'presigned-bytes' > "$W/signed"
awss3 cp "$W/signed" "s3://$B/signed.txt" >/dev/null
URL=$(awss3 presign "s3://$B/signed.txt")
BODY=$(curl -sS "$URL")
[ "$BODY" = "presigned-bytes" ] \
  && check "presigned GET returns the bytes" 1 \
  || check "presigned GET ($BODY)" 0

[ "$fails" -eq 0 ]
