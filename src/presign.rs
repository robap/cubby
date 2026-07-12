//! Server-side SigV4 query-string (presigned URL) generation.
//!
//! The web UI's presign button asks the server to mint a time-limited URL that
//! opens an object with no credentials. SigV4 stays server-side (CONCEPT: no
//! SigV4 in the UI).
//!
//! `s3s` 0.14 keeps its `sig_v4` helpers and `OrderedHeaders` in **private**
//! modules, so we can't call them. Instead we reproduce the SigV4 presigned
//! canonicalization exactly as `s3s`'s own verifier expects it — AWS
//! URI-encoding (unreserved `A-Za-z0-9-._~`, uppercase `%`-hex, `/` kept in the
//! path), a `host`-only signed-header set, and an `UNSIGNED-PAYLOAD` body hash —
//! so the URL we produce validates against this very server.
//!
//! **Sharp edge (documented, not solved):** the signed `Host` must equal the
//! host the browser hits. URLs signed for `localhost:9000` fail against
//! `http://tool:9000` inside Docker, and vice versa. We always sign the request
//! host, so a URL resolves against the instance that minted it.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

/// Inputs to [`presigned_url`]. `amz_date` is the SigV4 timestamp
/// (`YYYYMMDDTHHMMSSZ`); the caller supplies it so signing is deterministic and
/// testable.
pub struct PresignInput<'a> {
    pub method: &'a str,
    /// The host the browser will hit (must equal the signed host).
    pub host: &'a str,
    pub bucket: &'a str,
    pub key: &'a str,
    pub access_key: &'a str,
    pub secret_key: &'a str,
    pub region: &'a str,
    pub expires_in_s: u64,
    /// SigV4 timestamp, `YYYYMMDDTHHMMSSZ`.
    pub amz_date: &'a str,
}

/// Mint a presigned `http://{host}/{bucket}/{key}?…&X-Amz-Signature=…` URL that
/// this server's SigV4 verifier accepts.
pub fn presigned_url(input: &PresignInput<'_>) -> String {
    let service = "s3";
    let date = &input.amz_date[..8]; // YYYYMMDD
    let scope = format!("{date}/{}/{service}/aws4_request", input.region);
    let credential = format!("{}/{scope}", input.access_key);

    // The five presigned query parameters (decoded values), signed together.
    let params: [(&str, String); 5] = [
        ("X-Amz-Algorithm", "AWS4-HMAC-SHA256".to_owned()),
        ("X-Amz-Credential", credential.clone()),
        ("X-Amz-Date", input.amz_date.to_owned()),
        ("X-Amz-Expires", input.expires_in_s.to_string()),
        ("X-Amz-SignedHeaders", "host".to_owned()),
    ];

    // Canonical path: the decoded `/{bucket}/{key}`, AWS-encoded keeping `/`.
    let decoded_path = format!("/{}/{}", input.bucket, input.key);
    let canonical_path = uri_encode(&decoded_path, false);

    // Canonical query: each name=value AWS-encoded (encoding `/`), sorted by
    // encoded name.
    let mut encoded: Vec<(String, String)> = params
        .iter()
        .map(|(n, v)| (uri_encode(n, true), uri_encode(v, true)))
        .collect();
    encoded.sort_by(|a, b| a.0.cmp(&b.0));
    let canonical_query = encoded
        .iter()
        .map(|(n, v)| format!("{n}={v}"))
        .collect::<Vec<_>>()
        .join("&");

    // Canonical request (presigned form): host-only signed headers, unsigned
    // payload — matching s3s's verifier exactly.
    let canonical_request = format!(
        "{method}\n{canonical_path}\n{canonical_query}\nhost:{host}\n\nhost\nUNSIGNED-PAYLOAD",
        method = input.method,
        host = input.host,
    );

    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{scope}\n{}",
        input.amz_date,
        hex_sha256(canonical_request.as_bytes()),
    );

    let signature = sign(
        input.secret_key,
        date,
        input.region,
        service,
        string_to_sign.as_bytes(),
    );

    // Final URL: encoded path + encoded query + the signature.
    format!(
        "http://{host}{canonical_path}?{canonical_query}&X-Amz-Signature={signature}",
        host = input.host,
    )
}

/// AWS SigV4 URI-encode: keep `A-Za-z0-9-._~`; keep `/` unless `encode_slash`;
/// percent-encode everything else with **uppercase** hex.
fn uri_encode(input: &str, encode_slash: bool) -> String {
    let mut out = String::with_capacity(input.len());
    for &byte in input.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b'/' if !encode_slash => out.push('/'),
            _ => {
                out.push('%');
                out.push(HEX[(byte >> 4) as usize] as char);
                out.push(HEX[(byte & 0xf) as usize] as char);
            }
        }
    }
    out
}

const HEX: &[u8; 16] = b"0123456789ABCDEF";

/// The SigV4 signing key derivation + final HMAC, returning a lowercase hex
/// signature.
fn sign(secret: &str, date: &str, region: &str, service: &str, string_to_sign: &[u8]) -> String {
    let k_date = hmac(format!("AWS4{secret}").as_bytes(), date.as_bytes());
    let k_region = hmac(&k_date, region.as_bytes());
    let k_service = hmac(&k_region, service.as_bytes());
    let k_signing = hmac(&k_service, b"aws4_request");
    hex(&hmac(&k_signing, string_to_sign))
}

fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn hex_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex(&hasher.finalize())
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xf) as usize] as char);
    }
    s.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_encode_matches_aws_rules() {
        assert_eq!(uri_encode("a b", true), "a%20b");
        assert_eq!(uri_encode("a/b", false), "a/b");
        assert_eq!(uri_encode("a/b", true), "a%2Fb");
        // Unreserved set is preserved.
        assert_eq!(uri_encode("Aa0-_.~", true), "Aa0-_.~");
        // Uppercase hex.
        assert_eq!(uri_encode("é", true), "%C3%A9");
    }

    #[test]
    fn signing_key_matches_aws_known_answer() {
        // AWS SigV4 documented example: signing over the empty string yields a
        // published signature, confirming the HMAC key-derivation chain.
        // https://docs.aws.amazon.com/general/latest/gr/signature-v4-examples.html
        let sig = sign(
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            "20150830",
            "us-east-1",
            "iam",
            b"AWS4-HMAC-SHA256\n20150830T123600Z\n20150830/us-east-1/iam/aws4_request\nf536975d06c0309214f805bb90ccff089219ecd68b2577efef23edd43b7e1a59",
        );
        assert_eq!(
            sig,
            "33f5dad2191de0cb4b7ab912f876876c2c4f72e2991a458f9499233c7b992438"
        );
    }

    #[test]
    fn presigned_url_has_required_query_params() {
        let url = presigned_url(&PresignInput {
            method: "GET",
            host: "127.0.0.1:9000",
            bucket: "demo",
            key: "a/b.txt",
            access_key: "local",
            secret_key: "localsecret",
            region: "us-east-1",
            expires_in_s: 3600,
            amz_date: "20240101T000000Z",
        });
        assert!(url.starts_with("http://127.0.0.1:9000/demo/a/b.txt?"));
        assert!(url.contains("X-Amz-Algorithm=AWS4-HMAC-SHA256"));
        assert!(url.contains("X-Amz-Credential=local%2F20240101%2Fus-east-1%2Fs3%2Faws4_request"));
        assert!(url.contains("X-Amz-Expires=3600"));
        assert!(url.contains("X-Amz-SignedHeaders=host"));
        assert!(url.contains("&X-Amz-Signature="));
    }
}
