//! Pure listing engine for ListObjectsV2 / ListObjects v1.
//!
//! Listing is served entirely from the SQLite `objects` clustered index (see
//! [`crate::db::Db::list_objects_page`]); this module holds the *logic* that
//! sits on top of that seek primitive and is therefore testable with no DB or
//! server — just an in-memory key set. It owns:
//!
//! - [`successor`] — the exclusive upper bound of a prefix range, used both to
//!   bound the SQL scan and to skip-scan past a rolled-up delimiter group.
//! - continuation-token [`encode_token`] / [`decode_token`] — opaque base64 of
//!   the resume cursor.
//! - the engine itself ([`list_page`], added in a later box).
//!
//! **Ordering is BINARY (raw UTF-8 bytes)**, matching SQLite's default
//! collation on the TEXT `key` column and S3's own lexicographic order.

/// The least string strictly greater than *every* string that begins with
/// `prefix`, or `None` when no such string exists (an empty prefix, or a prefix
/// consisting entirely of the maximum scalar `U+10FFFF`).
///
/// Used as the **exclusive upper bound** of the prefix range `[prefix,
/// successor(prefix))` and as the **inclusive resume cursor** when a delimiter
/// group is rolled up into a `CommonPrefix` (jump past the whole group without
/// walking its members).
///
/// The increment is done at the Unicode-scalar level, not the byte level, so
/// the result is always valid UTF-8 while remaining a correct exclusive upper
/// bound under BINARY ordering — UTF-8 preserves code-point order, so bumping
/// the last scalar yields a string that sorts after all prefix members yet
/// before anything that does not share the prefix.
pub fn successor(prefix: &str) -> Option<String> {
    let mut chars: Vec<char> = prefix.chars().collect();
    while let Some(last) = chars.pop() {
        if let Some(next) = next_scalar(last) {
            let mut s: String = chars.into_iter().collect();
            s.push(next);
            return Some(s);
        }
        // `last` is U+10FFFF (the maximum scalar): drop it and carry into the
        // preceding character.
    }
    None
}

/// The next Unicode scalar after `c`, skipping the UTF-16 surrogate gap
/// (`U+D800..=U+DFFF`, which are not scalar values). `None` for `U+10FFFF`.
fn next_scalar(c: char) -> Option<char> {
    let n = c as u32 + 1;
    let n = if n == 0xD800 { 0xE000 } else { n };
    char::from_u32(n)
}

/// A continuation token that could not be decoded back into a resume cursor.
/// The handler maps this to `400 InvalidArgument`.
#[derive(Debug, thiserror::Error)]
#[error("malformed continuation token")]
pub struct TokenError;

/// Encode a resume cursor as an **opaque** continuation token: URL-safe base64
/// (no padding) of the cursor's UTF-8 bytes. Clients must treat it as opaque —
/// the base64 is an obfuscation contract, not an API.
pub fn encode_token(cursor: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(cursor.as_bytes())
}

/// Decode a continuation token back into its resume cursor. Fails with
/// [`TokenError`] if the token is not valid base64 or does not decode to UTF-8.
pub fn decode_token(token: &str) -> Result<String, TokenError> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(token.as_bytes())
        .map_err(|_| TokenError)?;
    String::from_utf8(bytes).map_err(|_| TokenError)
}

/// Percent-encode a key/prefix/delimiter for an `encoding-type=url` response so
/// XML-unsafe bytes (spaces, control chars, non-ASCII) survive the XML round
/// trip. Unreserved RFC 3986 characters plus `/` are left literal (matching how
/// S3 keeps path separators readable); everything else — including `%` — is
/// percent-encoded. Clients that see `EncodingType=url` percent-decode this back
/// to the exact stored key.
pub fn url_encode(s: &str) -> String {
    use percent_encoding::{percent_encode, AsciiSet, NON_ALPHANUMERIC};
    // Start from "encode everything non-alphanumeric" and exempt the RFC 3986
    // unreserved set and `/`.
    const SET: &AsciiSet = &NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'_')
        .remove(b'.')
        .remove(b'~')
        .remove(b'/');
    percent_encode(s.as_bytes(), SET).to_string()
}

/// Inputs to one page of a listing. `prefix`/`delimiter` are the S3 knobs;
/// `start_from` and `skip_cp_le` carry the resume state.
pub struct ListParams<'a> {
    /// Only keys beginning with this are listed (the fetch callback must honor
    /// the same bound; the engine re-uses it to strip and group).
    pub prefix: &'a str,
    /// When set (and non-empty), keys are rolled up to the first delimiter after
    /// the prefix into `CommonPrefixes`. Empty is treated as "no delimiter".
    pub delimiter: Option<&'a str>,
    /// Inclusive lower bound handed to the fetch callback — the resume cursor
    /// (v2 continuation token payload, or `"marker\0"` for v1). `None` starts at
    /// the beginning of the prefix range.
    pub start_from: Option<String>,
    /// v1-only: skip any `CommonPrefix` whose value is `<= this`. This is the
    /// client `marker`, and it makes marker-resume delimiter-aware — a key that
    /// rolls up into an already-returned group (whose prefix `<= marker`) is
    /// skipped instead of re-emitting the group. `None` for v2.
    pub skip_cp_le: Option<&'a str>,
    /// Maximum combined count of `Contents` + `CommonPrefixes` for this page.
    pub max_keys: usize,
}

/// One page of listing output. `T` is the caller's row type (e.g. `ObjectRow`).
pub struct ListPage<T> {
    /// Object rows that list as content, in ascending key order.
    pub contents: Vec<T>,
    /// Rolled-up delimiter groups, in ascending order, each ending in the
    /// delimiter.
    pub common_prefixes: Vec<String>,
    /// More items exist beyond this page.
    pub is_truncated: bool,
    /// v2: opaque continuation cursor (inclusive resume point) — `Some` iff
    /// truncated. Feed back as `start_from` for the next page.
    pub next_cursor: Option<String>,
    /// v1: `NextMarker` — the representative (last key or common prefix emitted)
    /// — `Some` iff truncated. The client passes it back as `marker`.
    pub next_marker: Option<String>,
}

/// How a single key lists, once its prefix is stripped and the delimiter (if
/// any) is applied.
enum Group {
    /// Lists as a content object.
    Content,
    /// Rolls up into this `CommonPrefix` (prefix + up to and including the first
    /// delimiter in the remainder).
    Prefix(String),
}

/// Classify `key` (guaranteed to begin with `prefix`) under `delimiter`.
fn classify(key: &str, prefix: &str, delimiter: Option<&str>) -> Group {
    let Some(delim) = delimiter.filter(|d| !d.is_empty()) else {
        return Group::Content;
    };
    let rest = &key[prefix.len()..];
    match rest.find(delim) {
        Some(idx) => Group::Prefix(key[..prefix.len() + idx + delim.len()].to_owned()),
        None => Group::Content,
    }
}

/// Run one page of a listing over the seek primitive `fetch`.
///
/// `fetch(from, limit)` must return up to `limit` rows whose key is `>= from`
/// (or from the start of the prefix range when `from` is `None`), already
/// bounded to the prefix and in ascending BINARY key order — exactly what
/// [`crate::db::Db::list_objects_page`] provides. `key_of` extracts a row's key.
///
/// The engine applies delimiter roll-up with **skip-scan** (on rolling a key up
/// into common prefix `P` it re-seeks the cursor to `successor(P)`, never
/// walking the rest of the group), counts keys and prefixes **together** toward
/// `max_keys`, and detects truncation by peeking one item past the page.
pub fn list_page<T: Clone>(
    mut fetch: impl FnMut(Option<&str>, i64) -> Vec<T>,
    key_of: impl Fn(&T) -> &str,
    params: &ListParams<'_>,
) -> ListPage<T> {
    let mut contents: Vec<T> = Vec::new();
    let mut common_prefixes: Vec<String> = Vec::new();
    let mut last_representative: Option<String> = None;
    let mut cursor: Option<String> = params.start_from.clone();

    // max_keys == 0 is a valid request for an empty, non-truncated page.
    if params.max_keys == 0 {
        return ListPage {
            contents,
            common_prefixes,
            is_truncated: false,
            next_cursor: None,
            next_marker: None,
        };
    }

    let emitted = |c: &[T], p: &[String]| c.len() + p.len();

    'outer: loop {
        if emitted(&contents, &common_prefixes) == params.max_keys {
            // Page is full; a single lookahead decides truncation.
            let more = !fetch(cursor.as_deref(), 1).is_empty();
            return ListPage {
                contents,
                common_prefixes,
                is_truncated: more,
                next_cursor: more.then(|| cursor.clone().unwrap_or_default()),
                next_marker: if more { last_representative } else { None },
            };
        }

        let want = (params.max_keys - emitted(&contents, &common_prefixes) + 1) as i64;
        let batch = fetch(cursor.as_deref(), want);
        let batch_len = batch.len();
        if batch_len == 0 {
            break; // prefix range exhausted
        }

        // `skip_below`: while skipping a rolled-up group, drop rows below the
        // group's successor without emitting them (in-batch skip-scan).
        let mut skip_below: Option<String> = None;

        for row in &batch {
            let key = key_of(row);
            if let Some(sb) = &skip_below {
                if key < sb.as_str() {
                    continue;
                }
                skip_below = None;
            }

            if emitted(&contents, &common_prefixes) == params.max_keys {
                // A real next item exists (this row) → truncated. `cursor` is the
                // inclusive resume point for exactly this row.
                return ListPage {
                    contents,
                    common_prefixes,
                    is_truncated: true,
                    next_cursor: Some(cursor.clone().unwrap_or_default()),
                    next_marker: last_representative,
                };
            }

            match classify(key, params.prefix, params.delimiter) {
                Group::Prefix(cp) => {
                    let Some(next) = successor(&cp) else {
                        break 'outer; // cp is unbounded (all-max) — nothing follows
                    };
                    // v1 marker-resume: a group already covered by `marker` is
                    // skipped rather than re-emitted.
                    let covered = params.skip_cp_le.is_some_and(|m| cp.as_str() <= m);
                    if !covered {
                        last_representative = Some(cp.clone());
                        common_prefixes.push(cp);
                    }
                    cursor = Some(next.clone());
                    skip_below = Some(next);
                }
                Group::Content => {
                    last_representative = Some(key.to_owned());
                    cursor = Some(format!("{key}\0"));
                    contents.push(row.clone());
                }
            }
        }

        if batch_len < want as usize {
            break; // fewer rows than asked ⇒ no more rows exist
        }
        // Full batch: loop and re-seek from the advanced cursor.
    }

    ListPage {
        contents,
        common_prefixes,
        is_truncated: false,
        next_cursor: None,
        next_marker: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Convenience: run the engine over a fixed key universe.
    fn run(all: &[&str], params: ListParams<'_>) -> ListPage<String> {
        // `all` must be sorted for the fetch to behave like the index.
        let mut sorted = all.to_vec();
        sorted.sort_unstable();
        let prefix = params.prefix.to_owned();
        list_page(
            mem_fetch_owned(sorted.iter().map(|s| s.to_string()).collect(), prefix),
            |s: &String| s.as_str(),
            &params,
        )
    }

    /// An in-memory fetch mimicking the DB primitive: keys within the prefix
    /// range, `>= from`, ascending, capped at `limit`.
    fn mem_fetch_owned(
        all: Vec<String>,
        prefix: String,
    ) -> impl FnMut(Option<&str>, i64) -> Vec<String> {
        let upper = successor(&prefix);
        move |from, limit| {
            all.iter()
                .filter(|k| k.starts_with(&prefix))
                .filter(|k| upper.as_deref().is_none_or(|u| k.as_str() < u))
                .filter(|k| from.is_none_or(|f| k.as_str() >= f))
                .take(limit.max(0) as usize)
                .cloned()
                .collect()
        }
    }

    fn params<'a>(prefix: &'a str, delimiter: Option<&'a str>, max_keys: usize) -> ListParams<'a> {
        ListParams {
            prefix,
            delimiter,
            start_from: None,
            skip_cp_le: None,
            max_keys,
        }
    }

    #[test]
    fn flat_list_no_delimiter_returns_all_in_order() {
        let keys = ["b", "a", "c/d", "c/e"];
        let page = run(&keys, params("", None, 100));
        assert_eq!(page.contents, ["a", "b", "c/d", "c/e"]);
        assert!(page.common_prefixes.is_empty());
        assert!(!page.is_truncated);
        assert_eq!(page.next_cursor, None);
    }

    #[test]
    fn delimiter_rolls_groups_and_keeps_siblings_as_content() {
        // "photos/index.md" is content beside common prefixes 2024/ and 2025/.
        let keys = [
            "notes.txt",
            "photos/index.md",
            "photos/2024/a.jpg",
            "photos/2024/b.jpg",
            "photos/2025/c.jpg",
        ];
        let page = run(&keys, params("photos/", Some("/"), 100));
        assert_eq!(page.contents, ["photos/index.md"]);
        assert_eq!(page.common_prefixes, ["photos/2024/", "photos/2025/"]);
        assert!(!page.is_truncated);
    }

    #[test]
    fn top_level_delimiter_groups_photos_and_lists_notes() {
        let keys = [
            "notes.txt",
            "photos/index.md",
            "photos/2024/a.jpg",
            "photos/2025/c.jpg",
        ];
        let page = run(&keys, params("", Some("/"), 100));
        assert_eq!(page.contents, ["notes.txt"]);
        assert_eq!(page.common_prefixes, ["photos/"]);
    }

    #[test]
    fn key_equal_to_prefix_lists_as_content() {
        // "a/" exactly equals the prefix ⇒ content, not a common prefix.
        let keys = ["a/", "a/b", "a/c/d"];
        let page = run(&keys, params("a/", Some("/"), 100));
        assert_eq!(page.contents, ["a/", "a/b"]);
        assert_eq!(page.common_prefixes, ["a/c/"]);
    }

    #[test]
    fn exact_boundary_is_not_truncated() {
        // Exactly max_keys items and nothing beyond ⇒ IsTruncated false.
        let keys = ["a", "b", "c"];
        let page = run(&keys, params("", None, 3));
        assert_eq!(page.contents, ["a", "b", "c"]);
        assert!(!page.is_truncated);
        assert_eq!(page.next_cursor, None);
    }

    #[test]
    fn truncation_mid_flat_list_resumes_exactly_once() {
        let keys = ["k0", "k1", "k2", "k3", "k4"];
        let mut seen = Vec::new();
        let mut cursor = None;
        loop {
            let mut p = params("", None, 2);
            p.start_from = cursor.clone();
            let page = run(&keys, p);
            seen.extend(page.contents.clone());
            if !page.is_truncated {
                assert_eq!(page.next_cursor, None);
                break;
            }
            cursor = page.next_cursor;
            assert!(cursor.is_some());
        }
        assert_eq!(
            seen,
            ["k0", "k1", "k2", "k3", "k4"],
            "every key once, in order"
        );
    }

    #[test]
    fn page_boundary_landing_mid_group_does_not_duplicate() {
        // Two groups; max_keys=1 forces a page break right after the first
        // common prefix, mid-way through the universe. Resume must not repeat it.
        let keys = ["p/2024/a", "p/2024/b", "p/2024/c", "p/2025/a", "p/2025/b"];
        let mut cps = Vec::new();
        let mut cursor = None;
        loop {
            let mut p = params("p/", Some("/"), 1);
            p.start_from = cursor.clone();
            let page = run(&keys, p);
            cps.extend(page.common_prefixes.clone());
            if !page.is_truncated {
                break;
            }
            cursor = page.next_cursor;
        }
        assert_eq!(cps, ["p/2024/", "p/2025/"], "each group once");
    }

    /// Rows fetched to list two groups, where the first group has `group_size`
    /// members. Skip-scan means this must NOT scale with `group_size`.
    fn reads_for_group(group_size: usize, max_keys: usize) -> usize {
        let mut keys: Vec<String> = (0..group_size).map(|i| format!("g/2024/{i:06}")).collect();
        keys.push("g/2025/only".to_owned());
        keys.sort_unstable();
        let upper = successor("g/");
        let mut rows_read = 0usize;
        let counting = |from: Option<&str>, limit: i64| -> Vec<String> {
            let v: Vec<String> = keys
                .iter()
                .filter(|k| k.starts_with("g/"))
                .filter(|k| upper.as_deref().is_none_or(|u| k.as_str() < u))
                .filter(|k| from.is_none_or(|f| k.as_str() >= f))
                .take(limit.max(0) as usize)
                .cloned()
                .collect();
            rows_read += v.len();
            v
        };
        let p = params("g/", Some("/"), max_keys);
        let page = list_page(counting, |s: &String| s.as_str(), &p);
        assert_eq!(page.common_prefixes, ["g/2024/", "g/2025/"]);
        rows_read
    }

    #[test]
    fn skip_scan_reads_are_bounded_by_page_not_group_size() {
        // A 100-member group and a 100_000-member group cost the same handful of
        // reads: the re-seek to successor(P) jumps past the whole group at the
        // index level instead of walking its members.
        let small = reads_for_group(100, 100);
        let huge = reads_for_group(100_000, 100);
        assert!(small <= 202, "small group read too many rows: {small}");
        assert!(
            huge <= 202,
            "skip-scan should not scale with group size: {huge} reads for 100k members"
        );
        assert!(
            huge.abs_diff(small) <= 2,
            "reads must be ~independent of group size: {small} vs {huge}"
        );
    }

    #[test]
    fn empty_and_no_match_are_empty_not_truncated() {
        let keys = ["a", "b"];
        let page = run(&keys, params("nope/", Some("/"), 100));
        assert!(page.contents.is_empty());
        assert!(page.common_prefixes.is_empty());
        assert!(!page.is_truncated);
    }

    #[test]
    fn max_keys_zero_is_empty_and_not_truncated() {
        let keys = ["a", "b", "c"];
        let page = run(&keys, params("", None, 0));
        assert!(page.contents.is_empty());
        assert!(!page.is_truncated);
        assert_eq!(page.next_cursor, None);
    }

    #[test]
    fn v1_marker_resume_skips_the_covered_group() {
        // Simulate v1 resume after common prefix "p/2024/": start_from is
        // "p/2024/\0" (strictly after the marker) and skip_cp_le is the marker,
        // so members of the 2024 group are not re-rolled into a duplicate.
        let keys = ["p/2024/a", "p/2024/b", "p/2025/a"];
        let mut sorted = keys.to_vec();
        sorted.sort_unstable();
        let marker = "p/2024/";
        let from = format!("{marker}\0");
        let p = ListParams {
            prefix: "p/",
            delimiter: Some("/"),
            start_from: Some(from),
            skip_cp_le: Some(marker),
            max_keys: 100,
        };
        let page = list_page(
            mem_fetch_owned(
                sorted.iter().map(|s| s.to_string()).collect(),
                "p/".to_owned(),
            ),
            |s: &String| s.as_str(),
            &p,
        );
        assert_eq!(page.common_prefixes, ["p/2025/"], "2024 group not repeated");
        assert!(page.contents.is_empty());
    }

    #[test]
    fn successor_of_ascii_increments_last_byte() {
        assert_eq!(successor("a").as_deref(), Some("b"));
        assert_eq!(successor("photos/").as_deref(), Some("photos0")); // '/'+1 == '0'
        assert_eq!(successor("ab").as_deref(), Some("ac"));
    }

    #[test]
    fn successor_bounds_the_prefix_range() {
        // Every string beginning with "ab" must sort < successor("ab").
        let sup = successor("ab").unwrap();
        for k in ["ab", "ab\0", "aba", "abz", "ab\u{10FFFF}"] {
            assert!(k < sup.as_str(), "{k:?} should be < {sup:?}");
        }
        // And the next non-member sorts >= the bound.
        assert!("ac" >= sup.as_str());
    }

    #[test]
    fn successor_carries_over_a_maxed_scalar() {
        // A trailing U+10FFFF has no next scalar, so the carry lands on the
        // previous character.
        let s = format!("a{}", '\u{10FFFF}');
        assert_eq!(successor(&s).as_deref(), Some("b"));
    }

    #[test]
    fn successor_of_all_max_is_none() {
        assert_eq!(successor(""), None);
        assert_eq!(successor("\u{10FFFF}"), None);
        assert_eq!(successor(&format!("{0}{0}", '\u{10FFFF}')), None);
    }

    #[test]
    fn successor_skips_the_surrogate_gap_and_stays_utf8() {
        // U+D7FF's next scalar must jump to U+E000, not a surrogate.
        let s = format!("x{}", '\u{D7FF}');
        let sup = successor(&s).unwrap();
        assert_eq!(sup, format!("x{}", '\u{E000}'));
        assert!(s.as_str() < sup.as_str());
    }

    #[test]
    fn token_round_trips_a_cursor() {
        for cursor in ["photos/2024/", "k01000\0", "a", "unicode/日本語/x"] {
            let tok = encode_token(cursor);
            assert_eq!(decode_token(&tok).unwrap(), cursor, "round-trip {cursor:?}");
        }
    }

    #[test]
    fn token_is_opaque_base64_not_the_raw_cursor() {
        // The wire form must not be the plaintext key (clients treat it opaque).
        let tok = encode_token("photos/2024/");
        assert_ne!(tok, "photos/2024/");
        assert!(!tok.contains('/'), "URL-safe alphabet has no '/': {tok}");
    }

    #[test]
    fn url_encode_encodes_unsafe_bytes_but_keeps_slash_and_unreserved() {
        assert_eq!(
            url_encode("my report (v2).txt"),
            "my%20report%20%28v2%29.txt"
        );
        assert_eq!(url_encode("photos/2024/a.jpg"), "photos/2024/a.jpg"); // '/' + unreserved kept
        assert_eq!(url_encode("a~b-c_d.e"), "a~b-c_d.e");
        assert_eq!(url_encode("100%"), "100%25"); // '%' itself is encoded
        assert_eq!(url_encode("日"), "%E6%97%A5"); // non-ASCII → UTF-8 bytes
                                                   // A raw control char that XML 1.0 cannot represent must be encoded.
        assert_eq!(url_encode("a\u{1}b"), "a%01b");
    }

    #[test]
    fn malformed_token_is_rejected() {
        // Not valid base64 (URL-safe alphabet excludes '*').
        assert!(decode_token("not*base64*").is_err());
        // Valid base64 of invalid UTF-8 bytes must also be rejected.
        use base64::Engine;
        let bad = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([0xff, 0xfe]);
        assert!(decode_token(&bad).is_err());
    }

    #[test]
    fn successor_result_is_valid_utf8_even_across_byte_length_change() {
        // U+007F (1 byte) -> U+0080 (2 bytes): still valid UTF-8 and a strict
        // upper bound in byte order.
        let s = "a\u{7F}";
        let sup = successor(s).unwrap();
        assert_eq!(sup, "a\u{80}");
        assert!(s < sup.as_str());
        // A byte-level increment (0x7F -> 0x80) would have produced invalid
        // UTF-8; the scalar-level increment does not.
        assert!(std::str::from_utf8(sup.as_bytes()).is_ok());
    }
}
