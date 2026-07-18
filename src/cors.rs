//! The pure CORS rule-matching engine — the fiddly, AWS-quirky part of the
//! per-bucket CORS feature, isolated and unit-tested the way [`crate::listing`]
//! and [`crate::multipart`] isolate theirs.
//!
//! A bucket stores an ordered list of [`CorsRule`]s (as JSON). The routing layer
//! (`http.rs`) consults this module at two points, both **first-match-wins**:
//!
//! - [`match_preflight`] answers an `OPTIONS` preflight: it needs the origin, the
//!   requested method, and the requested headers to all be covered by one rule.
//! - [`match_actual`] decides the `Access-Control-Allow-Origin` /
//!   `Access-Control-Expose-Headers` stamped onto a real cross-origin response;
//!   it needs only the origin and the request method to match.
//!
//! Origin matching mirrors S3: a bare `*` matches any origin (and yields a
//! literal `*` allow-origin); an entry with a single `*` wildcard segment
//! (`https://*.example.com`) matches by prefix/suffix and echoes the concrete
//! origin (with `Vary: Origin`); an exact string matches only itself (echoed).

use serde::{Deserialize, Serialize};

/// One bucket CORS rule. Serialized with the S3 `CORSRule` field names so the
/// stored JSON mirrors an `aws s3api put-bucket-cors --cors-configuration` body
/// and the read-only UI seam can surface it verbatim.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorsRule {
    #[serde(rename = "AllowedOrigins")]
    pub allowed_origins: Vec<String>,
    #[serde(rename = "AllowedMethods")]
    pub allowed_methods: Vec<String>,
    #[serde(
        rename = "AllowedHeaders",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub allowed_headers: Vec<String>,
    #[serde(
        rename = "ExposeHeaders",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub expose_headers: Vec<String>,
    #[serde(
        rename = "MaxAgeSeconds",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub max_age_seconds: Option<i32>,
    #[serde(rename = "ID", default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// What a matched preflight grants — the material for the `204` response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreflightGrant {
    /// `Access-Control-Allow-Origin`: a literal `*` (only for a bare-`*` rule) or
    /// the echoed request origin.
    pub allow_origin: String,
    /// `Access-Control-Allow-Methods`: the matched rule's allowed methods.
    pub allow_methods: Vec<String>,
    /// `Access-Control-Allow-Headers`: the requested headers, echoed (each was
    /// confirmed allowed). Empty when the preflight requested none.
    pub allow_headers: Vec<String>,
    /// `Access-Control-Max-Age`, if the rule set one.
    pub max_age_seconds: Option<i32>,
    /// `Access-Control-Expose-Headers`: the matched rule's expose list.
    pub expose_headers: Vec<String>,
    /// Whether to emit `Vary: Origin` — true for an echoed origin, false for `*`.
    pub vary_origin: bool,
}

/// What a matched actual (non-preflight) request grants — the CORS headers added
/// to the real S3 response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActualGrant {
    /// `Access-Control-Allow-Origin`: `*` or the echoed request origin.
    pub allow_origin: String,
    /// `Access-Control-Expose-Headers`: the matched rule's expose list.
    pub expose_headers: Vec<String>,
    /// Whether to emit `Vary: Origin`.
    pub vary_origin: bool,
}

/// Whether an `AllowedOrigins` entry matches a concrete request origin. A bare
/// `*` matches anything; an entry with one `*` matches by prefix + suffix
/// (`https://*.example.com` covers `https://a.example.com`); otherwise it is an
/// exact string comparison. Extra `*`s beyond the first are treated as literal —
/// S3 allows only one wildcard, and a client that writes more gets a stricter
/// (never looser) match.
fn origin_matches(pattern: &str, origin: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    match pattern.split_once('*') {
        Some((prefix, suffix)) => {
            origin.len() >= prefix.len() + suffix.len()
                && origin.starts_with(prefix)
                && origin.ends_with(suffix)
        }
        None => pattern == origin,
    }
}

/// The first `AllowedOrigins` entry of `rule` that matches `origin`, if any.
/// Returns the entry so the caller can tell a bare `*` (→ literal allow-origin)
/// from a specific/wildcard entry (→ echo the origin + `Vary`).
fn matching_origin<'a>(rule: &'a CorsRule, origin: &str) -> Option<&'a str> {
    rule.allowed_origins
        .iter()
        .map(String::as_str)
        .find(|p| origin_matches(p, origin))
}

/// The `Access-Control-Allow-Origin` value and whether `Vary: Origin` is needed:
/// a bare-`*` entry yields (`*`, no Vary); anything else echoes the concrete
/// origin (and needs Vary, since the response varies by request origin).
fn allow_origin_for(matched_pattern: &str, origin: &str) -> (String, bool) {
    if matched_pattern == "*" {
        ("*".to_owned(), false)
    } else {
        (origin.to_owned(), true)
    }
}

/// Whether `method` (uppercase, as the browser sends it) is in the rule's
/// allowed methods.
fn method_allowed(rule: &CorsRule, method: &str) -> bool {
    rule.allowed_methods.iter().any(|m| m == method)
}

/// Whether every requested header is covered by the rule: a `*` in
/// `AllowedHeaders` allows all; otherwise each requested header must appear in
/// the allowed list (case-insensitively, since HTTP header names are
/// case-insensitive). An empty request-header list is trivially covered.
fn headers_allowed(rule: &CorsRule, requested: &[String]) -> bool {
    if rule.allowed_headers.iter().any(|h| h == "*") {
        return true;
    }
    requested.iter().all(|req| {
        rule.allowed_headers
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(req))
    })
}

/// Match an `OPTIONS` preflight against a bucket's rules (first match wins). A
/// rule matches when it covers the origin **and** the requested method **and**
/// every requested header; `Some` carries the grant to render the `204`, `None`
/// means "refuse" (the caller returns `403` with no allow-origin, so the browser
/// blocks the pending request).
pub fn match_preflight(
    rules: &[CorsRule],
    origin: &str,
    method: &str,
    requested_headers: &[String],
) -> Option<PreflightGrant> {
    for rule in rules {
        let Some(pattern) = matching_origin(rule, origin) else {
            continue;
        };
        if !method_allowed(rule, method) || !headers_allowed(rule, requested_headers) {
            continue;
        }
        let (allow_origin, vary_origin) = allow_origin_for(pattern, origin);
        return Some(PreflightGrant {
            allow_origin,
            allow_methods: rule.allowed_methods.clone(),
            allow_headers: requested_headers.to_vec(),
            max_age_seconds: rule.max_age_seconds,
            expose_headers: rule.expose_headers.clone(),
            vary_origin,
        });
    }
    None
}

/// Match a real (non-preflight) request against a bucket's rules (first match
/// wins) to decide the CORS headers added to its response. A rule matches when
/// it covers the origin **and** the request method; `None` means no CORS headers
/// are added (cubby still serves the request — the browser is what withholds the
/// response from JS).
pub fn match_actual(rules: &[CorsRule], origin: &str, method: &str) -> Option<ActualGrant> {
    for rule in rules {
        let Some(pattern) = matching_origin(rule, origin) else {
            continue;
        };
        if !method_allowed(rule, method) {
            continue;
        }
        let (allow_origin, vary_origin) = allow_origin_for(pattern, origin);
        return Some(ActualGrant {
            allow_origin,
            expose_headers: rule.expose_headers.clone(),
            vary_origin,
        });
    }
    None
}

/// Parse a stored CORS rules JSON string into the domain rules. Used by the
/// enforcement path and the read-only seam.
pub fn parse_rules(json: &str) -> Result<Vec<CorsRule>, serde_json::Error> {
    serde_json::from_str(json)
}

/// The HTTP methods S3 allows in an `AllowedMethods` list.
pub const S3_METHODS: [&str; 5] = ["GET", "PUT", "POST", "DELETE", "HEAD"];

/// Validate a CORS configuration the way `PutBucketCors` must before storing it:
/// at least one rule, each rule with at least one origin and one method, and
/// every method in the S3 set. Returns a naming message on the first failure so
/// the store layer can reject the put with it and persist nothing.
pub fn validate(rules: &[CorsRule]) -> Result<(), String> {
    if rules.is_empty() {
        return Err("CORS configuration must contain at least one rule".to_owned());
    }
    for (i, rule) in rules.iter().enumerate() {
        if rule.allowed_origins.is_empty() {
            return Err(format!("rule {i} must have at least one AllowedOrigin"));
        }
        if rule.allowed_methods.is_empty() {
            return Err(format!("rule {i} must have at least one AllowedMethod"));
        }
        if let Some(bad) = rule
            .allowed_methods
            .iter()
            .find(|m| !S3_METHODS.contains(&m.as_str()))
        {
            return Err(format!(
                "rule {i} has an unsupported method: {bad} (allowed: {})",
                S3_METHODS.join(", ")
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(origins: &[&str], methods: &[&str]) -> CorsRule {
        CorsRule {
            allowed_origins: origins.iter().map(|s| s.to_string()).collect(),
            allowed_methods: methods.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    fn hdrs(hs: &[&str]) -> Vec<String> {
        hs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn bare_star_origin_matches_any_and_yields_literal_star() {
        let rules = vec![rule(&["*"], &["GET", "PUT"])];
        let grant = match_preflight(&rules, "http://anything.test", "PUT", &[]).unwrap();
        assert_eq!(grant.allow_origin, "*");
        // A `*` rule does not vary by origin, so no Vary: Origin.
        assert!(!grant.vary_origin);
        // Actual-request path agrees.
        let a = match_actual(&rules, "http://anything.test", "GET").unwrap();
        assert_eq!(a.allow_origin, "*");
        assert!(!a.vary_origin);
    }

    #[test]
    fn wildcard_subdomain_matches_and_echoes_origin() {
        let rules = vec![rule(&["https://*.example.com"], &["GET"])];
        let grant = match_preflight(&rules, "https://app.example.com", "GET", &[]).unwrap();
        // A wildcard-subdomain match echoes the concrete origin + varies on it.
        assert_eq!(grant.allow_origin, "https://app.example.com");
        assert!(grant.vary_origin);
        // A different apex does not match.
        assert!(match_preflight(&rules, "https://app.evil.com", "GET", &[]).is_none());
    }

    #[test]
    fn exact_origin_echoes_and_non_match_is_refused() {
        let rules = vec![rule(&["http://localhost:3000"], &["PUT"])];
        let grant = match_preflight(&rules, "http://localhost:3000", "PUT", &[]).unwrap();
        assert_eq!(grant.allow_origin, "http://localhost:3000");
        assert!(grant.vary_origin);
        // A non-listed origin is refused (browser would block).
        assert!(match_preflight(&rules, "http://evil.test", "PUT", &[]).is_none());
    }

    #[test]
    fn disallowed_method_is_refused() {
        // Rule allows only GET; a preflight for DELETE finds no match.
        let rules = vec![rule(&["http://localhost:3000"], &["GET"])];
        assert!(match_preflight(&rules, "http://localhost:3000", "DELETE", &[]).is_none());
        assert!(match_actual(&rules, "http://localhost:3000", "DELETE").is_none());
        // The allowed method still matches.
        assert!(match_preflight(&rules, "http://localhost:3000", "GET", &[]).is_some());
    }

    #[test]
    fn wildcard_headers_allow_anything() {
        let mut star = rule(&["*"], &["PUT"]);
        star.allowed_headers = hdrs(&["*"]);
        let rules = std::slice::from_ref(&star);
        // `*` covers any requested header, and they are echoed back as requested.
        let grant =
            match_preflight(rules, "o", "PUT", &hdrs(&["x-custom", "authorization"])).unwrap();
        assert_eq!(grant.allow_headers, hdrs(&["x-custom", "authorization"]));
    }

    #[test]
    fn named_header_list_is_case_insensitive() {
        let mut named = rule(&["*"], &["PUT"]);
        named.allowed_headers = hdrs(&["Content-Type", "Authorization"]);
        let rules = std::slice::from_ref(&named);
        // A named list matches case-insensitively…
        assert!(match_preflight(rules, "o", "PUT", &hdrs(&["content-type"])).is_some());
        assert!(match_preflight(rules, "o", "PUT", &hdrs(&["AUTHORIZATION"])).is_some());
        // …but a header outside the list is refused.
        assert!(match_preflight(rules, "o", "PUT", &hdrs(&["x-not-listed"])).is_none());
        // No requested headers → trivially covered.
        assert!(match_preflight(rules, "o", "PUT", &[]).is_some());
    }

    #[test]
    fn first_matching_rule_wins() {
        // Two rules cover the origin; the first that also covers the method wins,
        // and its methods/expose list are the ones returned.
        let mut r1 = rule(&["http://localhost:3000"], &["GET"]);
        r1.expose_headers = hdrs(&["X-First"]);
        let mut r2 = rule(&["http://localhost:3000"], &["GET", "PUT"]);
        r2.expose_headers = hdrs(&["X-Second"]);
        let rules = vec![r1, r2];

        // GET: rule 1 matches first → its expose list.
        let g = match_actual(&rules, "http://localhost:3000", "GET").unwrap();
        assert_eq!(g.expose_headers, hdrs(&["X-First"]));
        // PUT: rule 1 doesn't allow PUT, so rule 2 wins.
        let g = match_actual(&rules, "http://localhost:3000", "PUT").unwrap();
        assert_eq!(g.expose_headers, hdrs(&["X-Second"]));
    }

    #[test]
    fn max_age_and_expose_headers_carry_into_the_grant() {
        let mut r = rule(&["http://localhost:3000"], &["PUT"]);
        r.max_age_seconds = Some(600);
        r.expose_headers = hdrs(&["ETag"]);
        let grant = match_preflight(&[r], "http://localhost:3000", "PUT", &[]).unwrap();
        assert_eq!(grant.max_age_seconds, Some(600));
        assert_eq!(grant.expose_headers, hdrs(&["ETag"]));
    }

    #[test]
    fn validate_requires_rules_origins_methods_and_known_methods() {
        // Empty config is rejected.
        assert!(validate(&[]).is_err());
        // A rule with no origin, or no method, is rejected.
        assert!(validate(&[rule(&[], &["GET"])]).is_err());
        assert!(validate(&[rule(&["*"], &[])]).is_err());
        // An unknown method is rejected (naming it).
        let err = validate(&[rule(&["*"], &["FETCH"])]).unwrap_err();
        assert!(err.contains("FETCH"), "message names the bad method: {err}");
        // A well-formed config passes.
        assert!(validate(&[rule(&["http://localhost:3000"], &["GET", "PUT", "HEAD"])]).is_ok());
    }

    #[test]
    fn rules_round_trip_through_json_with_s3_field_names() {
        let json = r#"[{"AllowedOrigins":["http://localhost:3000"],"AllowedMethods":["GET","PUT"],"AllowedHeaders":["*"],"ExposeHeaders":["ETag"],"MaxAgeSeconds":600}]"#;
        let rules = parse_rules(json).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].allowed_methods, hdrs(&["GET", "PUT"]));
        assert_eq!(rules[0].max_age_seconds, Some(600));
        // Re-serializing preserves the S3 field names and drops empty optionals.
        let out = serde_json::to_string(&rules).unwrap();
        assert!(out.contains("\"AllowedOrigins\""));
        assert!(out.contains("\"ExposeHeaders\":[\"ETag\"]"));
        assert!(!out.contains("\"ID\""));
    }
}
