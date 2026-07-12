#!/usr/bin/env bash
#
# rclone conformance: a >8MB round-trip forced through rclone's multipart chunker,
# nested `rclone lsf` traversal (files + sub-prefix as a "directory"), and a
# `rclone link` presigned URL fetched by curl with no ambient credentials.
#
# The remote is configured entirely via RCLONE_CONFIG_CUBBY_* env vars — the
# path-style, plain-HTTP, static-cred config a user writes for a local S3.

set -uo pipefail
EP="$CUBBY_EP"; B="$CUBBY_BUCKET"; BIG="$CUBBY_BIG"; W="$CUBBY_WORK"

# On-the-fly remote "cubby:" — no config file needed.
export RCLONE_CONFIG=/dev/null   # suppress the "config not found" notice
export RCLONE_CONFIG_CUBBY_TYPE=s3
export RCLONE_CONFIG_CUBBY_PROVIDER=Other
export RCLONE_CONFIG_CUBBY_ENV_AUTH=false
export RCLONE_CONFIG_CUBBY_ACCESS_KEY_ID=local
export RCLONE_CONFIG_CUBBY_SECRET_ACCESS_KEY=localsecret
export RCLONE_CONFIG_CUBBY_ENDPOINT="$EP"
export RCLONE_CONFIG_CUBBY_REGION=us-east-1
export RCLONE_CONFIG_CUBBY_FORCE_PATH_STYLE=true
# Force multipart: a 1M cutoff sends the 12MB object as 5M+5M+2M chunks.
export RCLONE_CONFIG_CUBBY_UPLOAD_CUTOFF=1M
export RCLONE_CONFIG_CUBBY_CHUNK_SIZE=5M

fails=0
check() { if [ "$2" = 1 ]; then echo "ok  : $1"; else echo "FAIL: $1"; fails=$((fails + 1)); fi; }

rclone mkdir "cubby:$B" 2>/dev/null

# 1) >8MB round-trip through the multipart chunker, bytes verified equal.
rclone copyto "$BIG" "cubby:$B/big.bin" >/dev/null 2>&1
rclone copyto "cubby:$B/big.bin" "$W/rclone.dl" >/dev/null 2>&1
cmp -s "$BIG" "$W/rclone.dl" \
  && check "multipart >8MB round-trip: bytes equal" 1 \
  || check "multipart >8MB round-trip: bytes equal" 0

# 2) nested traversal. Upload a small tree, then list it two ways.
mkdir -p "$W/tree/docs/img"
printf a > "$W/tree/docs/a.txt"
printf b > "$W/tree/docs/b.txt"
printf c > "$W/tree/docs/img/c.txt"
printf t > "$W/tree/top.txt"
rclone copy "$W/tree" "cubby:$B" >/dev/null 2>&1

# Non-recursive lsf under docs/ shows the two files and the sub-prefix as a dir.
FOLDER=$(rclone lsf "cubby:$B/docs/" | sort | paste -sd, -)
[ "$FOLDER" = "a.txt,b.txt,img/" ] \
  && check "rclone lsf docs/ == a.txt,b.txt,img/" 1 \
  || check "rclone lsf docs/ ($FOLDER)" 0

# Recursive lsf yields every leaf key (order-independent).
REC=$(rclone lsf -R --files-only "cubby:$B" | sort | paste -sd, -)
case "$REC" in
  *"docs/a.txt"*docs/b.txt*docs/img/c.txt*top.txt*)
    check "rclone lsf -R lists all nested keys" 1 ;;
  *) check "rclone lsf -R lists all nested keys ($REC)" 0 ;;
esac

# 3) rclone link → a presigned URL curl fetches to the bytes, no creds.
printf 'presigned-bytes' > "$W/signed.txt"
rclone copyto "$W/signed.txt" "cubby:$B/signed.txt" >/dev/null 2>&1
URL=$(rclone link "cubby:$B/signed.txt" 2>/dev/null)
if [ -z "$URL" ]; then
  check "rclone link produced a URL" 0
else
  BODY=$(curl -sS "$URL")
  [ "$BODY" = "presigned-bytes" ] \
    && check "rclone link URL fetches the bytes" 1 \
    || check "rclone link URL fetches the bytes ($BODY)" 0
fi

[ "$fails" -eq 0 ]
