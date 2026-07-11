# Listing & delimiter semantics — spec

**Status:** done · **Roadmap:** Phase 2 · **Slug:** 02-listing-delimiters

## Why

Every S3 client's second move (after "can I put/get bytes?") is "show me
what's here." `aws s3 ls`, the web UI's bucket browser, and every sync tool
all sit on top of one endpoint: **ListObjectsV2** (plus legacy **ListObjects
v1**). It is the most quirk-laden surface in the whole API — prefix filtering,
delimiter-driven "folders" (`CommonPrefixes`), lexicographic ordering, and
opaque pagination all have to be exactly right or clients silently drop
objects, loop forever, or refuse to sync.

`rclone` is the canary here: it is brutal about delimiter correctness and
pagination, so if rclone can list, traverse, and `sync` nested prefixes
without error, the "folders work in every client" promise holds.

North stars served:
- **The filesystem is the API too** — but listing is served from **SQLite,
  not `readdir`**. The `objects` table is `WITHOUT ROWID` with
  `PK(bucket, key)`, so it *is* the clustered index we scan in key order;
  `readdir` gives neither correct lexicographic order nor cheap skip-scan.
- **Compatibility is proven, not claimed** — `rclone` (and the AWS CLI) drive
  the acceptance, not hand-rolled request assertions.
- **Dev tool first** — correct ordering and pagination so a human `ls` and a
  CI `sync` both behave like real S3.

## In scope

- **ListObjectsV2** (`GET /<bucket>?list-type=2`) with:
  - `prefix` — restrict to keys starting with it.
  - `delimiter` — arbitrary string (commonly `/`); roll keys sharing a
    substring up to the next delimiter into `CommonPrefixes`.
  - `max-keys` — page size; default 1000, **capped at 1000** even if a larger
    value is requested.
  - `continuation-token` — opaque resume cursor for the next page.
  - `start-after` — begin strictly after this key (first request only).
  - `encoding-type=url` — URL-encode keys/prefixes/delimiter in the response
    so XML-illegal bytes round-trip.
  - `fetch-owner` — include `Owner` in `Contents` when `true`.
  - Response fields: `Name`, `Prefix`, `Delimiter`, `MaxKeys`, `KeyCount`
    (count of `Contents` + `CommonPrefixes` returned), `IsTruncated`,
    `ContinuationToken` (echo), `NextContinuationToken`, `StartAfter`,
    `EncodingType`, and `Contents`/`CommonPrefixes`.
- **ListObjects v1** (`GET /<bucket>`) with `prefix`, `delimiter`, `max-keys`,
  `marker`, `encoding-type`. Response uses `Marker`/`NextMarker` (the latter
  present only when a `delimiter` is set, per S3) instead of continuation
  tokens.
- **CommonPrefixes via skip-scan** over the clustered `objects` index: on
  hitting the first key of a delimiter group, emit the prefix once and jump the
  cursor past the whole group (successor-key bound) rather than scanning every
  member.
- **Lexicographic (UTF-8 byte) ordering**, matching SQLite's default `BINARY`
  collation on the TEXT key column — same order S3 uses.
- **Per-object listing fields:** `Key`, `LastModified`, `ETag` (quoted hex
  MD5), `Size`, `StorageClass=STANDARD`, and `Owner` when requested.
- Both endpoints served entirely from SQLite; no `readdir`.

## Out of scope

- **Multipart / composite ETags** → Phase 3. (Listing returns whatever ETag is
  stored; multipart objects list correctly once Phase 3 stores their ETags.)
- **Presigned/query-string auth, CopyObject, DeleteObjects batch** → Phase 4.
  Listing is header-SigV4 authed like Phase 1.
- **`ListObjectVersions`, versioning, object lock, tagging** → CONCEPT
  non-goals; not in any milestone.
- **`RequestPayer`, `ExpectedBucketOwner`, `OptionalObjectAttributes`** — parsed
  by `s3s` but accepted-and-ignored.
- **Web UI bucket browser** → Phase 5. This phase makes the S3 endpoint correct;
  the UI's `/_/api/` list endpoint is built on it later.

## Behavior

- **Addressing:** path-style only (`GET http://127.0.0.1:9000/<bucket>?...`),
  consistent with Phase 1.
- **Missing bucket:** either endpoint on a non-existent bucket → `404
  NoSuchBucket`.
- **Empty result:** empty bucket, or a `prefix` matching nothing → `200` with
  no `Contents`, no `CommonPrefixes`, `KeyCount` 0 (v2), `IsTruncated` false.
- **Delimiter grouping:** for each key, strip `prefix`, then find the first
  `delimiter` in the remainder. If present, the substring up to and including
  that delimiter (re-prefixed) is a `CommonPrefix`; the key itself is **not**
  in `Contents`. If absent, the key is a `Contents` entry. A key that equals
  the prefix exactly, or has no delimiter after the prefix, lists as content.
- **Ordering & interleaving:** results are produced by a single ascending scan
  over keys, so `Contents` and `CommonPrefixes` are each in lexicographic order
  and a common prefix sorts at the position of its first member. `max-keys`
  counts keys and common prefixes **together**; a page may end in the middle of
  what would otherwise be one logical group.
- **Pagination (v2):** when the scan is cut short by `max-keys`, `IsTruncated`
  is true and `NextContinuationToken` encodes the resume point (the last key or
  common-prefix emitted). The token is **opaque** (base64) — clients must not
  parse it. Feeding it back as `continuation-token` resumes with the next
  greater key, yielding every matching key/prefix **exactly once**, in order,
  across pages. The final page has `IsTruncated` false and no
  `NextContinuationToken`. A malformed/undecodable token → `400 InvalidArgument`.
- **Pagination (v1):** same scan, but the cursor is the plaintext `marker`
  (start strictly after it). `NextMarker` is returned **only when `delimiter`
  is set** (S3 quirk); without a delimiter the client is expected to use the
  last `Key` of `Contents` as the next `marker`.
- **`start-after` (v2):** first page begins strictly after the given key; keys
  ≤ it are excluded. Ignored when a `continuation-token` is also supplied.
- **`max-keys` bounds:** default 1000; values > 1000 are silently capped to
  1000; `max-keys=0` → empty page, `IsTruncated` false. Negative → `400
  InvalidArgument`.
- **`encoding-type=url`:** when requested, `Key`, `Prefix`, `Delimiter`,
  `StartAfter`, and `CommonPrefixes` in the response are URL-encoded so keys
  containing spaces, control chars, or other XML-unsafe bytes round-trip. The
  echoed `EncodingType` is `url`. (Keys are stored/compared as their canonical
  form; encoding is presentation-only.)
- **Owner:** returned when `fetch-owner=true` (v2) or always (v1), with a fixed
  dev identity (`ID` = display name = the configured access key). We have no
  real IAM; a stable owner keeps SDKs that read it happy.
- **StorageClass:** always `STANDARD`.
- **Consistency with the filesystem:** a key visible via `ls`/`sync` has a real
  row *and* real bytes on disk from Phase 1's write path; listing reflects the
  SQLite source of truth (an orphaned file with no row does not appear).

## Acceptance criteria

Named client is **rclone** (the delimiter/pagination canary), with the **AWS
CLI** for precise field assertions. Fixture unless noted: a bucket `photos`
seeded (via Phase 1 `put-object`) with keys `notes.txt`, `photos/index.md`,
`photos/2024/a.jpg`, `photos/2024/b.jpg`, `photos/2025/c.jpg`. Each becomes a
checkbox in the plan.

- [ ] `rclone lsf remote:photos` (delimiter `/`) → exactly `notes.txt` and
      `photos/` (a dir marker), nothing else.
- [ ] `rclone lsf -R remote:photos` → all five keys, flat, in lexicographic
      order, no dir markers.
- [ ] `rclone sync remote:photos ./dl` → exits 0, no errors; `./dl` contains
      every object at its full key path and `cmp` against the originals is
      clean. A second `rclone sync` reports "0 transferred" (listing is stable).
- [ ] `rclone lsf remote:photos/photos/2024/` → `a.jpg` and `b.jpg` only
      (nested-prefix traversal).
- [ ] `aws s3 ls s3://photos/` → one `PRE photos/` line plus `notes.txt`.
- [ ] `aws s3api list-objects-v2 --bucket photos --prefix photos/ --delimiter /`
      → `CommonPrefixes` = `[photos/2024/, photos/2025/]`, `Contents` =
      `[photos/index.md]`, `KeyCount` = 3.
- [ ] **Pagination:** seed a bucket with 2500 keys (`k00000`…`k02499`).
      `aws s3api list-objects-v2 --bucket p --max-keys 1000` → `IsTruncated`
      true with a `NextContinuationToken`; manually following the token three
      times yields exactly 2500 keys, in order, no dupes, last page
      `IsTruncated` false. `aws s3 ls s3://p --recursive` (auto-paginates) →
      2500 lines.
- [ ] **max-keys cap:** `--max-keys 5000` against the 2500-key bucket returns
      ≤ 1000 entries with `IsTruncated` true.
- [ ] **start-after:** `aws s3api list-objects-v2 --bucket p --start-after k01000`
      → first key returned is `k01001`.
- [ ] **Legacy v1:** `aws s3api list-objects --bucket photos --delimiter /`
      returns `CommonPrefixes` and, when truncated, a `NextMarker` that resumes
      correctly; the same call without `--delimiter` omits `NextMarker`.
- [ ] **encoding-type / weird keys:** put a key `my report (v2).txt`;
      `aws s3api list-objects-v2 --bucket photos --encoding-type url` returns it
      URL-encoded in the XML, and `rclone lsf remote:photos` shows the decoded
      real name and can download it.
- [ ] **Empty/no-match:** `aws s3api list-objects-v2 --bucket photos --prefix
      nope/` → no `Contents`/`CommonPrefixes`, `KeyCount` 0, `IsTruncated`
      false.

## Open questions

Proposed defaults below; flagged for review, not blocking the criteria. Adopt
unless you disagree.

- **Continuation-token format.** ✅ Opaque base64 of the last emitted
  key/prefix (clients treat it as opaque anyway). Malformed → `400
  InvalidArgument`. Alternative (HMAC-signed token to detect tampering) is
  over-engineering for a dev tool.
- **Owner identity when `fetch-owner`/v1.** ✅ Fixed `ID` = `DisplayName` = the
  configured access key. Revisit only if a client validates canonical user IDs.
- **`delimiter` other than `/`.** ✅ Support arbitrary delimiter strings (S3
  does); costs nothing given the skip-scan is delimiter-generic.
- **Skip-scan vs. naive scan.** ✅ Implement the successor-key skip so a bucket
  with millions of keys under one prefix lists the prefix without scanning the
  group. If it complicates the first cut, a naive per-key scan is a correct
  fallback (same output, worse constant) — but the clustered index exists
  precisely to make skip-scan cheap.
