# Listing & delimiter semantics — plan

**Status:** done · **Spec:** [02-listing-delimiters-spec.md](02-listing-delimiters-spec.md) · **Roadmap:** Phase 2

## Approach

Listing is served entirely from SQLite's `objects` table, which is `WITHOUT
ROWID` with `PK(bucket, key)` — the table *is* the clustered index we scan in
UTF-8 byte order (SQLite's default `BINARY` collation matches S3's ordering).
This serves *filesystem-is-the-API-too*: `readdir` gives neither correct
lexicographic order nor cheap delimiter skip-scan, so we never touch it here.

The work splits into three testable pieces, each a north-star bet on *dev-tool
test ergonomics*:

1. **A DB seek primitive** (`db.rs`) — "give me up to N object rows for bucket
   B whose key is `> cursor` and starts with `prefix`, in key order." One
   bounded, index-backed query.
2. **A pure listing engine** (`listing.rs`) — given that primitive as a
   callback, it applies prefix filtering, delimiter roll-up into
   `CommonPrefixes`, `max-keys` counting (keys **and** prefixes count together),
   truncation detection, and **skip-scan**: on rolling a key up into common
   prefix `P`, it re-seeks the cursor to `successor(P)` (byte-increment) rather
   than walking every member. Being callback-driven, it unit-tests against an
   in-memory key set with no DB or server.
3. **Opaque continuation tokens** (`listing.rs`) — base64 of the exclusive
   "resume-after key" (a real key for a content hit, `successor(P)` for a prefix
   hit). Clients treat it as opaque; malformed → `InvalidArgument`.

The `s3s` trait methods `list_objects_v2` / `list_objects` (v1) are thin
adapters: apply the `max-keys` default(1000)/cap(1000) that `s3s` does *not*
apply for us, pick the cursor (`continuation-token` > `start-after`/`marker`),
call the engine, then map its result into the DTO — adding `StorageClass=
STANDARD`, `Owner` when asked, `KeyCount`, `IsTruncated`, and the next
token/marker. `s3s` 0.14.1 serializes the `EncodingType` field but does **not**
percent-encode keys, so when `encoding-type=url` is set we URL-encode `Key`,
`Prefix`, `Delimiter`, `StartAfter`, and each `CommonPrefix` ourselves —
presentation only; stored keys are untouched. Per-step TDD mirrors Phase 1:
unit tests for the pure engine/token/successor logic, in-process `aws-sdk-s3`
integration tests for the handlers, and **rclone** + AWS CLI via `/verify` for
Acceptance.

## Files

- `src/listing.rs` — **new.** Pure engine: `list_page(fetch, prefix, delimiter,
  cursor, max_keys) -> ListPage { contents, common_prefixes, is_truncated,
  next_cursor }`; `successor(prefix)` byte-increment helper; token
  encode/decode; `encode_key`/url-encode helper. Unit tests.
- `src/db.rs` — add `list_objects_page(bucket, prefix, after_exclusive, limit)
  -> Vec<ObjectRow>` (index-backed, key-ordered, prefix-bounded). Unit tests
  for order, prefix bound, and cursor exclusivity.
- `src/store.rs` — implement `list_objects_v2` and `list_objects`; wire the
  engine, max-keys default/cap, cursor precedence, owner/storage-class, and
  encoding-type.
- `src/lib.rs` — `mod listing;`.
- `tests/s3_api.rs` — integration tests driving `aws-sdk-s3` listing.
- `README.md` — document ListObjectsV2 + legacy v1 support (Docs step).

## Risks & unknowns

- **`successor(P)` byte-carry.** Incrementing the last byte must handle a
  trailing `0xFF` (drop it, carry to the previous byte); an all-`0xFF` suffix
  means "no upper bound / scan to end." Keys are Rust `String` (UTF-8); do the
  increment on bytes, not chars, to match `BINARY` collation. Unit-test the
  carry corners or skip-scan silently drops keys.
- **Ordering must be `BINARY`, not a Unicode collation.** Confirm the `key`
  column has no `COLLATE` override (it doesn't in schema v0) so SQLite compares
  raw UTF-8 bytes exactly as S3 does. A test with mixed-case/`_`/`~` keys pins
  this (`A` < `_` < `a` < `~` by byte).
- **`max-keys` handling is entirely ours.** `s3s` passes the raw `Option<i32>`
  through — no default, no cap, no validation. We apply default 1000, cap 1000,
  `0` → empty non-truncated, negative → `InvalidArgument`.
- **`encoding-type=url` is ours too.** `s3s` won't percent-encode keys; a key
  with an XML-illegal byte (e.g. `\x01`) would otherwise produce invalid XML.
  Encode in the handler when requested.
- **v1 `NextMarker` quirk.** Present only when `delimiter` is set; without a
  delimiter the client reuses the last `Key`. Encode this exactly or clients
  loop or truncate.
- **Truncation detection.** Fetch one item beyond `max_keys` (or track that the
  seek returned a full batch with more behind) to set `IsTruncated` without an
  off-by-one — the classic listing bug. Cover the exact-boundary case (total ==
  max_keys → not truncated) in tests.

## Steps

Each box ≈ one small commit moving an observable behavior. Check only when the
outcome is real, not when code is written.

- [x] **DB seek primitive** — `Db::list_objects_page(bucket, prefix, from,
      limit)` returns key-ordered rows in `[prefix, successor(prefix))` with an
      **inclusive** `from` lower bound (`key >= from`), via the clustered index.
      Unit tests: BINARY order, prefix bound excludes non-matches, `from`
      inclusivity (+ `"K\0"` = strictly-after), `limit`/`0`, bucket scoping.
- [x] **`successor`** (pure, unit-tested) — scalar-level increment so the
      exclusive prefix upper bound / skip-scan resume cursor is always valid
      UTF-8 yet correct under BINARY order; `None` when the prefix is empty or
      all-max. Carry, surrogate-gap, and byte-length-change corners tested.
      *(Pulled ahead of the token codec: the DB primitive needs it.)*
- [x] **token codec** (pure, unit-tested) — token encode/decode round-trips a
      cursor through opaque URL-safe base64; malformed base64 *or* non-UTF-8 →
      `TokenError`. Unit tests incl. opacity + both malformed forms.
- [x] **Listing engine** (pure, unit-tested) — `list_page` over an in-memory
      fetch callback produces correct `Contents`/`CommonPrefixes` with
      delimiter roll-up, skip-scan re-seek, combined `max-keys` counting, and
      truncation + `next_cursor`/`next_marker`. Unit tests: flat list; delimiter
      grouping; key beside a common prefix; key == prefix → content; page
      boundary mid-group (no dup); exact-boundary non-truncation; skip-scan
      bounded by page not group size; empty/no-match; max-keys 0; v1
      marker-resume skip. *(`ListParams.skip_cp_le` added for v1's delimiter-
      aware marker resume; engine returns both `next_cursor` (v2) and
      `next_marker` (v1).)*
- [x] **ListObjectsV2 handler** — `list_objects_v2` wires the engine: max-keys
      default 1000/cap 1000/`0`-empty/negative-`InvalidArgument`; cursor
      precedence (`continuation-token` over `start-after`); builds `Contents`
      (ETag, Size, LastModified, `StorageClass=STANDARD`), `CommonPrefixes`,
      `KeyCount`, `IsTruncated`, `NextContinuationToken`, echoes
      `Name/Prefix/Delimiter/MaxKeys/StartAfter`; `Owner` when `fetch-owner`.
      Integration tests (`aws-sdk-s3`): prefix+delimiter grouping (KeyCount 3),
      top-level grouping, recursive order, 2500-key 3-page round-trip (every key
      once, in order), max-keys cap, start-after, fetch-owner, empty/no-match,
      negative max-keys + bad token → `InvalidArgument`, missing bucket.
- [x] **encoding-type=url** — `listing::url_encode` percent-encodes `Key`,
      `Prefix`, `Delimiter`, `StartAfter`, and each `CommonPrefix` (unreserved +
      `/` kept literal); `EncodingType=url` echoed; stored keys unchanged.
      Integration test: `my report (v2).txt` lists as
      `my%20report%20%28v2%29.txt` under url, literally without it. *(Note:
      `aws-sdk-s3` returns the raw encoded key — it does not auto-decode; rclone
      decodes it, proven in Acceptance.)*
- [x] **ListObjects v1 handler** — `list_objects` reuses the engine with a
      `marker` cursor (`start_from = "marker\0"`, `skip_cp_le = marker`); returns
      `NextMarker` only when `delimiter` is set, echoes `Marker`, always includes
      `Owner`; same encoding-type handling. Integration tests: delimiter
      grouping + owner; NextMarker present-with-delimiter / absent-without across
      a full marker-resumed round trip (each group once); marker strictly-after;
      missing bucket.
- [x] **Missing-bucket + edge sweep** — both endpoints on a nonexistent bucket
      → `NoSuchBucket` (v2 + v1 tests); empty/no-match → empty, `KeyCount` 0,
      `IsTruncated` false (v2 `nope/` test). Covered by the per-box integration
      tests above; no gaps found.
- [x] **Docs** — `README.md` gains a "Listing (Phase 2)" section (prefix,
      delimiter, max-keys default/cap, continuation-token/marker, start-after,
      encoding-type, fetch-owner, SQLite-backed BINARY ordering); ListObjectsV2
      removed from "Not yet implemented".

## Progress notes

- **`successor` pulled into box 1.** The DB seek primitive's prefix upper bound
  needs it, so it was implemented (with full unit tests) alongside box 1 rather
  than in box 2; box 2 shrank to just the token codec.
- **`successor` is scalar-level, not byte-level.** The plan's Risks assumed a
  byte increment with `0xFF` carry. A raw byte increment can produce invalid
  UTF-8 (e.g. `"a\x7F"` → `"a\x80"`), which cannot bind as a SQLite TEXT bound
  or flow through a `String` cursor. Incrementing the last Unicode scalar (with
  surrogate-gap skip and max-scalar carry) keeps the result valid UTF-8 while
  staying a correct exclusive upper bound under BINARY order.
- **Inclusive `from` cursor + `\0` trick.** The DB primitive takes an inclusive
  `from` (`key >= from`), not `after_exclusive`. Content resume is encoded as
  `"key\0"` (strictly-after) and group skip-scan as `successor(P)` (inclusive),
  so one bound serves both without an off-by-one.
- **`ListParams.skip_cp_le` for v1.** v1's plaintext `marker` can't represent a
  post-group resume exactly, so the engine additionally skips any `CommonPrefix
  <= marker` — S3's delimiter-aware marker semantics — preventing a resumed page
  from re-emitting a group. The engine returns both `next_cursor` (v2 token) and
  `next_marker` (v1).
- **Acceptance harness quirks (not product bugs).** botocore auto-paginates
  `list-objects-v2`, stripping per-page scalars (`KeyCount`/`IsTruncated`) from
  the merged result — field asserts use `--no-paginate`. `rclone lsf -R`
  synthesizes directory entries — the flat-keys check uses `--files-only`. The
  wire XML is correct (verified via `aws --debug` and the aws-sdk-s3 tests).

## Acceptance

Mirrors the spec. `/implement` isn't done until every box passes by driving the
named client — **rclone** (delimiter/pagination canary) + the **AWS CLI** for
field-level asserts — against a live `buckit serve --port 0`. Fixture bucket
`photos`: `notes.txt`, `photos/index.md`, `photos/2024/a.jpg`,
`photos/2024/b.jpg`, `photos/2025/c.jpg`.

All boxes verified by a live harness (`tests/acceptance/listing.sh`) driving
real rclone + AWS CLI against `buckit serve --port 0`: **16/16 checks PASS**.

- [x] `rclone lsf remote:photos` → exactly `notes.txt` and `photos/`.
- [x] `rclone lsf -R remote:photos` → all five keys, flat, lexicographic, no
      dir markers. *(`--files-only`: rclone otherwise prints synthetic dirs.)*
- [x] `rclone sync remote:photos ./dl` → exit 0, no errors; every object lands
      at its full key path, `cmp` clean; a second `sync` transfers 0.
- [x] `rclone lsf remote:photos/photos/2024/` → `a.jpg` and `b.jpg` only.
- [x] `aws s3 ls s3://photos/` → one `PRE photos/` line plus `notes.txt`.
- [x] `aws s3api list-objects-v2 --bucket photos --prefix photos/ --delimiter /`
      → `CommonPrefixes` = `[photos/2024/, photos/2025/]`, `Contents` =
      `[photos/index.md]`, `KeyCount` = 3.
- [x] 2500-key bucket (`k00000`…`k02499`): `list-objects-v2 --max-keys 1000` →
      `IsTruncated` true + `NextContinuationToken`; following the token yields
      exactly 2500 keys, in order, no dupes, last page not truncated;
      `aws s3 ls s3://p --recursive` → 2500 lines.
- [x] `list-objects-v2 --max-keys 5000` on that bucket → ≤ 1000 entries,
      `IsTruncated` true.
- [x] `list-objects-v2 --bucket p --start-after k01000` → first key `k01001`.
- [x] `aws s3api list-objects --bucket photos --delimiter /` → `CommonPrefixes`
      + a resuming `NextMarker` when truncated; without `--delimiter`,
      `NextMarker` is absent.
- [x] key `my report (v2).txt`: `list-objects-v2 --encoding-type url` returns it
      URL-encoded in XML; `rclone lsf remote:photos` shows the decoded name and
      downloads it.
- [x] `list-objects-v2 --bucket photos --prefix nope/` → no
      `Contents`/`CommonPrefixes`, `KeyCount` 0, `IsTruncated` false.
