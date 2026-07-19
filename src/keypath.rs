//! Canonical S3 key → filesystem path derivation.
//!
//! The filesystem is the API too, so a key lands as a real file at a *derived*
//! path under `buckets/<bucket>/`. The mapping is **encode-only**: we never
//! decode a filename back into a key (SQLite holds the canonical key). Rules:
//!
//! - Split the key on `/`; each segment becomes one path component, so nested
//!   prefixes create nested directories.
//! - Percent-encode the Windows-illegal set (`< > : " | ? * \`), control
//!   characters, and `%` itself (kept for an injective mapping).
//! - Percent-encode trailing dots/spaces in a segment (illegal on Windows).
//! - Percent-encode the first character of reserved device names (`CON`,
//!   `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`), with or without an
//!   extension.
//!
//! The trailing-dot rule also neutralizes `.` and `..` segments (they become
//! `%2E` / `%2E%2E`), so a crafted key can never traverse out of its bucket.

use std::path::{Component, Path, PathBuf};

use percent_encoding::{percent_decode_str, percent_encode, AsciiSet, CONTROLS};

/// Characters that are illegal in filenames on Windows (and `%`, so the
/// encoding is reversible in principle / injective in practice). `CONTROLS`
/// already covers `0x00`–`0x1F` and `0x7F`.
const ILLEGAL: &AsciiSet = &CONTROLS
    .add(b'<')
    .add(b'>')
    .add(b':')
    .add(b'"')
    .add(b'|')
    .add(b'?')
    .add(b'*')
    .add(b'\\')
    .add(b'%');

/// Derive the bucket-relative filesystem path for a canonical object key.
pub fn key_to_relpath(key: &str) -> PathBuf {
    let mut path = PathBuf::new();
    for segment in key.split('/') {
        path.push(encode_segment(segment));
    }
    path
}

/// Recover the canonical S3 key from a bucket-relative filesystem path — the
/// exact inverse of [`key_to_relpath`]. Split the path into components,
/// percent-decode each one, and join with `/`.
///
/// This is **the one place cubby decodes a filename back into a key** (the
/// sanctioned reindex exception to CONCEPT's "never decode filenames back into
/// keys" serving rule). The inverse is exact because `key_to_relpath` is
/// encode-only and injective: `%` is always encoded, so no `%XX` on disk is
/// ambiguous. For a plain drop-in file with no `%XX` (the common case) the
/// inverse is the identity — "what I copied in is the key".
pub fn relpath_to_key(rel: &Path) -> String {
    rel.components()
        .filter_map(|c| match c {
            Component::Normal(os) => Some(os.to_string_lossy()),
            // A bucket-relative path holds only normal components (the encoder
            // neutralizes `.`/`..`); ignore anything else defensively.
            _ => None,
        })
        .map(|comp| {
            percent_decode_str(comp.as_ref())
                .decode_utf8_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Encode one `/`-delimited key segment into a safe path component.
fn encode_segment(segment: &str) -> String {
    let encoded = percent_encode(segment.as_bytes(), ILLEGAL).to_string();
    let encoded = encode_trailing(&encoded);
    encode_reserved(encoded)
}

/// Percent-encode any trailing `.`/` ` run (illegal at the end of a Windows
/// name). Handles multiple trailing chars, which is what defuses `.`/`..`.
fn encode_trailing(s: &str) -> String {
    let trimmed = s.trim_end_matches(['.', ' ']);
    if trimmed.len() == s.len() {
        return s.to_string();
    }
    let mut out = String::from(trimmed);
    for c in s[trimmed.len()..].chars() {
        match c {
            '.' => out.push_str("%2E"),
            ' ' => out.push_str("%20"),
            _ => unreachable!("trim_end_matches only strips '.' and ' '"),
        }
    }
    out
}

/// If the name's stem (before the first `.`) is a reserved device name,
/// percent-encode its first byte so the filename is no longer reserved.
fn encode_reserved(s: String) -> String {
    let stem = s.split('.').next().unwrap_or("");
    if is_reserved_stem(stem) {
        let first = s.as_bytes()[0];
        format!("%{:02X}{}", first, &s[1..])
    } else {
        s
    }
}

fn is_reserved_stem(stem: &str) -> bool {
    let up = stem.to_ascii_uppercase();
    if matches!(up.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    let b = up.as_bytes();
    b.len() == 4
        && (up.starts_with("COM") || up.starts_with("LPT"))
        && b[3].is_ascii_digit()
        && b[3] != b'0'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(key: &str) -> String {
        key_to_relpath(key).to_string_lossy().into_owned()
    }

    #[test]
    fn plain_key_is_unchanged() {
        assert_eq!(rel("report.pdf"), "report.pdf");
    }

    #[test]
    fn nested_prefixes_become_nested_dirs() {
        assert_eq!(
            key_to_relpath("photos/cat.jpg"),
            PathBuf::from("photos").join("cat.jpg")
        );
    }

    #[test]
    fn windows_illegal_chars_are_percent_encoded() {
        // `weird:name?.txt` — no raw `:` or `?` on disk.
        let out = rel("weird:name?.txt");
        assert_eq!(out, "weird%3Aname%3F.txt");
        assert!(!out.contains(':'));
        assert!(!out.contains('?'));
    }

    #[test]
    fn full_illegal_set_encoded() {
        assert_eq!(rel("a<>:\"|?*\\b"), "a%3C%3E%3A%22%7C%3F%2A%5Cb");
    }

    #[test]
    fn percent_is_encoded_for_injectivity() {
        assert_eq!(rel("100%.txt"), "100%25.txt");
        // A literal `%3A` key must not collide with the encoding of `:`.
        assert_eq!(rel("%3A"), "%253A");
        assert_ne!(rel("%3A"), rel(":"));
    }

    #[test]
    fn control_chars_encoded() {
        assert_eq!(rel("a\tb\nc"), "a%09b%0Ac");
    }

    #[test]
    fn trailing_dot_and_space_encoded_per_segment() {
        assert_eq!(rel("name."), "name%2E");
        assert_eq!(rel("name "), "name%20");
        assert_eq!(rel("a. ./b"), "a%2E%20%2E/b");
        // A dot in the middle stays literal.
        assert_eq!(rel("v1.2.3"), "v1.2.3");
    }

    #[test]
    fn dot_and_dotdot_segments_are_neutralized() {
        assert_eq!(rel("."), "%2E");
        assert_eq!(rel(".."), "%2E%2E");
    }

    #[test]
    fn path_traversal_cannot_escape() {
        // The classic attack must not yield a real `..` component.
        let p = key_to_relpath("../../etc/passwd");
        assert_eq!(
            p,
            PathBuf::from("%2E%2E")
                .join("%2E%2E")
                .join("etc")
                .join("passwd")
        );
        assert!(!p
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir)));
    }

    #[test]
    fn reserved_device_names_encoded() {
        assert_eq!(rel("CON"), "%43ON");
        assert_eq!(rel("con"), "%63on");
        assert_eq!(rel("CON.txt"), "%43ON.txt");
        assert_eq!(rel("COM1"), "%43OM1");
        assert_eq!(rel("LPT9.log"), "%4CPT9.log");
        assert_eq!(rel("nul"), "%6Eul");
        // Not reserved: COM0, plain words containing the token.
        assert_eq!(rel("COM0"), "COM0");
        assert_eq!(rel("CONTROL"), "CONTROL");
        assert_eq!(rel("console"), "console");
    }

    #[test]
    fn relpath_to_key_is_the_exact_inverse() {
        // Every tricky case must survive a full key → path → key round-trip,
        // proving reindex recovers the original key from bytes alone.
        for key in [
            "report.pdf",
            "photos/cat.jpg",
            "a:b",
            "100%.txt",
            "CON",
            "name.",
            "..",
            "weird:name?.txt",
            "a<>:\"|?*\\b",
            "v1.2.3",
            "a. ./b",
            "%3A",
        ] {
            let rel = key_to_relpath(key);
            assert_eq!(
                relpath_to_key(&rel),
                key,
                "round-trip failed for {key:?} (path {rel:?})"
            );
        }
    }

    #[test]
    fn relpath_to_key_of_a_plain_path_is_identity() {
        // A hand-dropped file with no `%XX` maps to itself — "the key is the path".
        assert_eq!(
            relpath_to_key(&PathBuf::from("photos").join("cat.jpg")),
            "photos/cat.jpg"
        );
        assert_eq!(relpath_to_key(&PathBuf::from("report.pdf")), "report.pdf");
    }

    #[test]
    fn distinct_keys_map_to_distinct_paths() {
        // Injectivity spot-check across the tricky cases.
        let keys = [
            "a:b", "a%3Ab", "CON", "con", "name.", "name", ".", "..", "a/b", "a%2Fb",
        ];
        let paths: Vec<_> = keys.iter().map(|k| rel(k)).collect();
        for i in 0..paths.len() {
            for j in (i + 1)..paths.len() {
                assert_ne!(paths[i], paths[j], "collision: {} vs {}", keys[i], keys[j]);
            }
        }
    }
}
