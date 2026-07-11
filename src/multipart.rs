//! Pure multipart helpers — the fiddly, order-sensitive bits of the multipart
//! lifecycle, kept free of any DB or filesystem so they can be unit-tested in
//! isolation (the same split as [`crate::listing`]).
//!
//! Three concerns live here:
//!
//! - **Composite ETag** ([`composite_etag`]) — the `md5-of-md5s-N` value real S3
//!   returns for a multipart object. The classic bug is hashing the wrong thing:
//!   it is the MD5 of the concatenated **raw 16-byte** part digests (decoded
//!   from the hex we recorded at UploadPart — never recomputed from the bytes),
//!   suffixed `-<part count>`. A single part still yields `-1`.
//! - **ETag normalization** ([`normalize_etag`]) — a client echoes each part's
//!   ETag back in the Complete list, possibly quoted or as a weak validator;
//!   strip it to the bare hex before comparing.
//! - **Part-list validation** ([`validate_complete`]) — the client's Complete
//!   list must be non-empty, strictly ascending, and every entry must match a
//!   recorded part by number and ETag.
//!
//! [`new_upload_id`] mints the opaque, filesystem-safe token that names both the
//! upload and its `.multipart/<upload_id>/` staging directory.

use md5::{Digest, Md5};

/// A part as recorded at UploadPart: its number, byte size, and hex MD5.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedPart {
    pub part_number: i32,
    pub size: i64,
    /// Hex MD5 (unquoted) of the part's bytes.
    pub etag_hex: String,
}

/// A part as submitted by the client in the CompleteMultipartUpload list.
#[derive(Debug, Clone)]
pub struct SubmittedPart {
    pub part_number: i32,
    /// The ETag exactly as the client sent it (may be quoted / weak).
    pub etag: String,
}

/// Why a CompleteMultipartUpload part list was rejected. Each maps to a distinct
/// S3 wire error code (see [`crate::store`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompleteError {
    /// The submitted list was empty → `InvalidRequest`.
    Empty,
    /// Parts were not in strictly ascending `PartNumber` order → `InvalidPartOrder`.
    OutOfOrder,
    /// A submitted part does not exist, or its ETag does not match the recorded
    /// one → `InvalidPart` (names the offending part number).
    InvalidPart { part_number: i32 },
}

/// The S3 composite ETag for a completed multipart object: the hex MD5 of the
/// concatenated **raw** 16-byte part digests, suffixed `-<count>`.
///
/// `part_md5_hex` are the parts' recorded hex MD5s in ascending part order. A
/// single part still gets a `-1` suffix (this is how real S3 marks multipart
/// objects, and how sync tools tell them apart from single-PUT objects).
///
/// # Panics
/// Panics if a supplied string is not valid MD5 hex. Callers pass values we
/// produced at UploadPart via `hex::encode`, so a malformed digest is a broken
/// internal invariant, not a client error.
pub fn composite_etag(part_md5_hex: &[&str]) -> String {
    let mut hasher = Md5::new();
    for h in part_md5_hex {
        let raw = hex::decode(h).expect("recorded part MD5 is valid hex");
        hasher.update(&raw);
    }
    format!("{}-{}", hex::encode(hasher.finalize()), part_md5_hex.len())
}

/// Strip an ETag down to its bare value for comparison: remove a leading weak
/// marker (`W/`), surrounding double quotes, and trimming whitespace. The
/// recorded part ETags are unquoted lowercase hex; comparison against the
/// normalized client value is case-insensitive (see [`validate_complete`]).
pub fn normalize_etag(etag: &str) -> String {
    let s = etag.trim();
    let s = s.strip_prefix("W/").unwrap_or(s);
    let s = s.trim();
    s.strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(s)
        .to_owned()
}

/// Validate a client's Complete part list against the recorded parts, returning
/// the selected recorded parts in ascending order (ready to assemble).
///
/// All checks run before any assembly (the spec's ordering guarantee):
/// non-empty, strictly ascending part numbers, and every submitted part matching
/// a recorded part by number and (normalized, case-insensitive) ETag. Recorded
/// parts the client omits are dropped — S3 permits completing a subset.
pub fn validate_complete(
    submitted: &[SubmittedPart],
    recorded: &[RecordedPart],
) -> Result<Vec<RecordedPart>, CompleteError> {
    if submitted.is_empty() {
        return Err(CompleteError::Empty);
    }
    // Strictly ascending part numbers (rejects duplicates and descending order).
    for w in submitted.windows(2) {
        if w[1].part_number <= w[0].part_number {
            return Err(CompleteError::OutOfOrder);
        }
    }

    let mut selected = Vec::with_capacity(submitted.len());
    for part in submitted {
        let Some(rec) = recorded.iter().find(|r| r.part_number == part.part_number) else {
            return Err(CompleteError::InvalidPart {
                part_number: part.part_number,
            });
        };
        if !normalize_etag(&part.etag).eq_ignore_ascii_case(&rec.etag_hex) {
            return Err(CompleteError::InvalidPart {
                part_number: part.part_number,
            });
        }
        selected.push(rec.clone());
    }
    Ok(selected)
}

/// Mint an opaque, filesystem-safe upload id: 32 hex chars from 16 random bytes.
/// Hex contains no path separators, so it is safe as the `.multipart/<id>/`
/// directory name, and 128 bits makes collisions between concurrent uploads
/// vanishingly unlikely. Clients treat it as opaque.
pub fn new_upload_id() -> String {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).expect("OS randomness is available");
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(part_number: i32, etag: &str) -> SubmittedPart {
        SubmittedPart {
            part_number,
            etag: etag.to_owned(),
        }
    }

    fn rec(part_number: i32, etag_hex: &str) -> RecordedPart {
        RecordedPart {
            part_number,
            size: 1,
            etag_hex: etag_hex.to_owned(),
        }
    }

    #[test]
    fn composite_etag_matches_known_vector() {
        // Vector computed independently with Python's hashlib:
        //   part1 = b"a"*10, part2 = b"b"*20
        //   md5(md5(part1).digest() + md5(part2).digest()).hexdigest() + "-2"
        let p1 = "e09c80c42fda55f9d992e59ca6b3307d";
        let p2 = "085a8cdaee82caf894e995cd72b220bb";
        assert_eq!(
            composite_etag(&[p1, p2]),
            "5775febf235ca234938723332e97075f-2"
        );
    }

    #[test]
    fn composite_etag_single_part_still_has_dash_one() {
        let p1 = "e09c80c42fda55f9d992e59ca6b3307d";
        assert_eq!(composite_etag(&[p1]), "ac1f8aff40d191e27d29b7848a8d9abd-1");
    }

    #[test]
    fn composite_is_not_md5_of_concatenated_hex_strings() {
        // Guard against the classic bug: hashing the hex text, not raw digests.
        let p1 = "e09c80c42fda55f9d992e59ca6b3307d";
        let p2 = "085a8cdaee82caf894e995cd72b220bb";
        let wrong = format!(
            "{}-2",
            hex::encode(Md5::digest(format!("{p1}{p2}").as_bytes()))
        );
        assert_ne!(composite_etag(&[p1, p2]), wrong);
    }

    #[test]
    fn normalize_strips_quotes_and_weak_marker() {
        assert_eq!(normalize_etag("\"abc\""), "abc");
        assert_eq!(normalize_etag("W/\"abc\""), "abc");
        assert_eq!(normalize_etag("abc"), "abc");
        assert_eq!(normalize_etag("  \"abc\"  "), "abc");
    }

    #[test]
    fn validate_rejects_empty() {
        assert_eq!(
            validate_complete(&[], &[rec(1, "aa")]),
            Err(CompleteError::Empty)
        );
    }

    #[test]
    fn validate_rejects_out_of_order_and_duplicates() {
        let recorded = [rec(1, "aa"), rec(2, "bb")];
        assert_eq!(
            validate_complete(&[sub(2, "bb"), sub(1, "aa")], &recorded),
            Err(CompleteError::OutOfOrder)
        );
        assert_eq!(
            validate_complete(&[sub(1, "aa"), sub(1, "aa")], &recorded),
            Err(CompleteError::OutOfOrder)
        );
    }

    #[test]
    fn validate_rejects_missing_part() {
        let recorded = [rec(1, "aa")];
        assert_eq!(
            validate_complete(&[sub(1, "aa"), sub(2, "bb")], &recorded),
            Err(CompleteError::InvalidPart { part_number: 2 })
        );
    }

    #[test]
    fn validate_rejects_mismatched_etag() {
        let recorded = [rec(1, "aa")];
        assert_eq!(
            validate_complete(&[sub(1, "ffff")], &recorded),
            Err(CompleteError::InvalidPart { part_number: 1 })
        );
    }

    #[test]
    fn validate_accepts_ascending_subset_ignoring_quotes() {
        // Client may complete a subset (parts 1 and 3), in ascending order, and
        // may quote the ETags. Recorded part 2 is simply dropped.
        let recorded = [rec(1, "aa"), rec(2, "bb"), rec(3, "cc")];
        let selected = validate_complete(&[sub(1, "\"aa\""), sub(3, "\"CC\"")], &recorded).unwrap();
        assert_eq!(
            selected.iter().map(|p| p.part_number).collect::<Vec<_>>(),
            [1, 3]
        );
    }

    #[test]
    fn new_upload_id_is_nonempty_and_separator_free() {
        let id = new_upload_id();
        assert_eq!(id.len(), 32);
        assert!(!id.contains('/'));
        assert!(!id.contains('\\'));
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
        // Two ids differ (collision-resistant).
        assert_ne!(id, new_upload_id());
    }
}
