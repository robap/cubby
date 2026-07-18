//! Event notifications via webhook — the `Notifier` and its pure core.
//!
//! When an object is created or removed, cubby can POST a JSON event to a
//! per-bucket destination URL, shaped like the event AWS would deliver — so the
//! handler a developer writes against cubby runs unchanged in prod (see
//! `docs/features/event-notifications-spec.md`).
//!
//! This module keeps the fiddly, order-sensitive decisions **pure** and unit
//! tested — the S3 `eventName`/family/reason/detail-type mappings, event-list
//! matching, prefix/suffix filtering, and write-time validation — separate from
//! the delivery engine (background POST) added later. The same split as
//! [`crate::multipart`] and [`crate::listing`].

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use http_body_util::Full;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;

use crate::db::{Db, NotificationRow};
use crate::events::{Auth, EventBus, EventDraft};

/// The object-lifecycle events cubby fires webhooks for. Resolved at the
/// storage-mutation points (not the wire), so each carries the committed
/// object's post-state where it has one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    /// A successful `PutObject`.
    Put,
    /// A successful `CopyObject` (for the destination key).
    Copy,
    /// A completed multipart object.
    CompleteMultipartUpload,
    /// A successful `DeleteObject`, or one per removed key in `DeleteObjects`.
    Delete,
}

impl EventKind {
    /// The **S3-notification** `eventName` field value — note S3 drops the
    /// `s3:` prefix here (`ObjectCreated:Put`), unlike the config filter names.
    pub fn event_name(self) -> &'static str {
        match self {
            EventKind::Put => "ObjectCreated:Put",
            EventKind::Copy => "ObjectCreated:Copy",
            EventKind::CompleteMultipartUpload => "ObjectCreated:CompleteMultipartUpload",
            EventKind::Delete => "ObjectRemoved:Delete",
        }
    }

    /// The **config-filter** name, `s3:`-prefixed (`s3:ObjectCreated:Put`) — how
    /// a destination's `events` list names a specific event.
    pub fn qualified_name(self) -> &'static str {
        match self {
            EventKind::Put => "s3:ObjectCreated:Put",
            EventKind::Copy => "s3:ObjectCreated:Copy",
            EventKind::CompleteMultipartUpload => "s3:ObjectCreated:CompleteMultipartUpload",
            EventKind::Delete => "s3:ObjectRemoved:Delete",
        }
    }

    /// The wildcard family this event belongs to (`s3:ObjectCreated:*` or
    /// `s3:ObjectRemoved:*`), which a destination may subscribe to instead of a
    /// specific event.
    pub fn family(self) -> &'static str {
        if self.is_created() {
            "s3:ObjectCreated:*"
        } else {
            "s3:ObjectRemoved:*"
        }
    }

    /// The **EventBridge** coarse `detail-type` (`Object Created` /
    /// `Object Deleted`).
    pub fn detail_type(self) -> &'static str {
        if self.is_created() {
            "Object Created"
        } else {
            "Object Deleted"
        }
    }

    /// The **EventBridge** `detail.reason` — the specific API that caused it.
    pub fn reason(self) -> &'static str {
        match self {
            EventKind::Put => "PutObject",
            EventKind::Copy => "CopyObject",
            EventKind::CompleteMultipartUpload => "CompleteMultipartUpload",
            EventKind::Delete => "DeleteObject",
        }
    }

    /// Whether this is a creation event (carries `size`/`eTag`) rather than a
    /// removal (which omits them).
    pub fn is_created(self) -> bool {
        !matches!(self, EventKind::Delete)
    }
}

/// One resolved object mutation, ready to be matched against a bucket's
/// destinations and rendered into a payload. `size`/`etag` are present for
/// creations and absent for removals (`ObjectRemoved` omits them, matching AWS).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectEvent {
    pub bucket: String,
    pub key: String,
    pub kind: EventKind,
    pub size: Option<i64>,
    /// Hex MD5 / composite ETag (unquoted); `None` for removals.
    pub etag: Option<String>,
}

impl ObjectEvent {
    /// A creation event carrying the committed object's size and ETag.
    pub fn created(bucket: &str, key: &str, kind: EventKind, size: i64, etag: &str) -> Self {
        Self {
            bucket: bucket.to_owned(),
            key: key.to_owned(),
            kind,
            size: Some(size),
            etag: Some(etag.to_owned()),
        }
    }

    /// A removal event (no size/ETag).
    pub fn removed(bucket: &str, key: &str) -> Self {
        Self {
            bucket: bucket.to_owned(),
            key: key.to_owned(),
            kind: EventKind::Delete,
            size: None,
            etag: None,
        }
    }
}

/// Every event token a destination's `events` list may contain: the two
/// wildcard families plus each specific event.
pub const KNOWN_EVENTS: &[&str] = &[
    "s3:ObjectCreated:*",
    "s3:ObjectCreated:Put",
    "s3:ObjectCreated:Copy",
    "s3:ObjectCreated:CompleteMultipartUpload",
    "s3:ObjectRemoved:*",
    "s3:ObjectRemoved:Delete",
];

/// The two supported payload formats.
pub const FORMATS: &[&str] = &["s3-notification", "eventbridge"];

/// The default per-destination delivery timeout (milliseconds) when none is set.
pub const DEFAULT_TIMEOUT_MS: i64 = 5000;

/// Whether a destination subscribed to `events` fires for `kind` — it matches
/// when the list names the specific event (`s3:ObjectCreated:Put`) **or** its
/// wildcard family (`s3:ObjectCreated:*`).
pub fn event_subscribed(events: &[String], kind: EventKind) -> bool {
    events
        .iter()
        .any(|e| e == kind.qualified_name() || e == kind.family())
}

/// Whether `key` passes a destination's optional prefix/suffix filters — each
/// absent means no constraint, exactly as S3's `FilterRule` prefix/suffix
/// behave.
pub fn key_passes_filters(key: &str, prefix: Option<&str>, suffix: Option<&str>) -> bool {
    prefix.is_none_or(|p| key.starts_with(p)) && suffix.is_none_or(|s| key.ends_with(s))
}

/// Whether a fully-parsed destination (its `events`/`prefix`/`suffix`) should
/// fire for `event`: the event is subscribed **and** the key passes the filters.
pub fn destination_matches(
    events: &[String],
    prefix: Option<&str>,
    suffix: Option<&str>,
    event: &ObjectEvent,
) -> bool {
    event_subscribed(events, event.kind) && key_passes_filters(&event.key, prefix, suffix)
}

/// Validate a destination at **write time** (the seam's POST): an `http://` url
/// (not `https://`, out of scope for v0.2), a non-empty `events` list of only
/// known tokens, a known `format`, and a positive `timeout_ms`. Returns a
/// naming error message on the first failure, so the seam can 400 with it and
/// persist nothing.
pub fn validate_destination(
    url: &str,
    events: &[String],
    format: &str,
    timeout_ms: i64,
) -> Result<(), String> {
    if !url.starts_with("http://") {
        return Err(format!(
            "url must be an http:// URL (https:// is not supported in v0.2): {url}"
        ));
    }
    if events.is_empty() {
        return Err("events must not be empty".to_owned());
    }
    if let Some(bad) = events.iter().find(|e| !KNOWN_EVENTS.contains(&e.as_str())) {
        return Err(format!(
            "unknown event: {bad} (known: {})",
            KNOWN_EVENTS.join(", ")
        ));
    }
    if !FORMATS.contains(&format) {
        return Err(format!(
            "format must be one of: {} (got {format})",
            FORMATS.join(", ")
        ));
    }
    if timeout_ms <= 0 {
        return Err(format!(
            "timeout_ms must be a positive integer (got {timeout_ms})"
        ));
    }
    Ok(())
}

/// The dev/placeholder context a payload needs that isn't on the event itself:
/// the (accepted-and-ignored) region, the ISO-8601 event time, the local
/// monotonic `sequencer` (hex), and a generated `id` (EventBridge only). The
/// [`Notifier`] fills these; keeping them as inputs keeps the builders pure.
pub struct PayloadCtx<'a> {
    pub region: &'a str,
    pub event_time: &'a str,
    pub sequencer: &'a str,
    pub id: &'a str,
}

/// URL-encode an object key the way S3 encodes it in the **S3-notification**
/// `s3.object.key` field: unreserved RFC 3986 chars and `/` stay literal (paths
/// remain readable), spaces become `+`, everything else is percent-encoded — so
/// a handler that url-decodes the key keeps working against cubby.
fn encode_notification_key(key: &str) -> String {
    use percent_encoding::{percent_encode, AsciiSet, NON_ALPHANUMERIC};
    const SET: &AsciiSet = &NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'_')
        .remove(b'.')
        .remove(b'~')
        .remove(b'/');
    percent_encode(key.as_bytes(), SET)
        .to_string()
        .replace("%20", "+")
}

/// Render an [`ObjectEvent`] as the **S3-notification** `{"Records":[…]}`
/// envelope (what SQS/SNS/Lambda-direct integrations receive). Creation events
/// carry `size`/`eTag`; removals omit both. `eventName` drops the `s3:` prefix,
/// the key is url-encoded, and fields cubby has no real value for are stable dev
/// placeholders.
pub fn s3_notification_payload(event: &ObjectEvent, ctx: &PayloadCtx) -> serde_json::Value {
    use serde_json::json;

    let mut object = serde_json::Map::new();
    object.insert("key".to_owned(), json!(encode_notification_key(&event.key)));
    if let Some(size) = event.size {
        object.insert("size".to_owned(), json!(size));
    }
    if let Some(etag) = &event.etag {
        object.insert("eTag".to_owned(), json!(etag));
    }
    object.insert("sequencer".to_owned(), json!(ctx.sequencer));

    json!({
        "Records": [
            {
                "eventVersion": "2.1",
                "eventSource": "aws:s3",
                "awsRegion": ctx.region,
                "eventTime": ctx.event_time,
                "eventName": event.kind.event_name(),
                "s3": {
                    "s3SchemaVersion": "1.0",
                    "configurationId": "cubby",
                    "bucket": {
                        "name": event.bucket,
                        "arn": format!("arn:aws:s3:::{}", event.bucket),
                    },
                    "object": object,
                }
            }
        ]
    })
}

/// Render an [`ObjectEvent`] as the **EventBridge** `{source, detail-type,
/// detail}` envelope (what the S3 → EventBridge path delivers). Note the field
/// is lowercase `etag` here — an AWS inconsistency cubby reproduces so handlers
/// port cleanly — and `detail.reason` names the specific API.
pub fn eventbridge_payload(event: &ObjectEvent, ctx: &PayloadCtx) -> serde_json::Value {
    use serde_json::json;

    let mut object = serde_json::Map::new();
    object.insert("key".to_owned(), json!(event.key));
    if let Some(size) = event.size {
        object.insert("size".to_owned(), json!(size));
    }
    if let Some(etag) = &event.etag {
        object.insert("etag".to_owned(), json!(etag));
    }
    object.insert("sequencer".to_owned(), json!(ctx.sequencer));

    json!({
        "version": "0",
        "id": ctx.id,
        "detail-type": event.kind.detail_type(),
        "source": "aws.s3",
        "account": "000000000000",
        "time": ctx.event_time,
        "region": ctx.region,
        "resources": [format!("arn:aws:s3:::{}", event.bucket)],
        "detail": {
            "version": "0",
            "bucket": { "name": event.bucket },
            "object": object,
            "reason": event.kind.reason(),
        }
    })
}

/// Render `event` in the destination's `format` (`eventbridge` selects the
/// EventBridge shape; anything else — including the default — is the
/// S3-notification shape).
pub fn render_payload(format: &str, event: &ObjectEvent, ctx: &PayloadCtx) -> serde_json::Value {
    if format == "eventbridge" {
        eventbridge_payload(event, ctx)
    } else {
        s3_notification_payload(event, ctx)
    }
}

/// cubby's outbound webhook client: a plain HTTP (no TLS) `hyper_util` legacy
/// client. `http://`-only keeps cubby's first outbound-HTTP capability to the
/// `hyper` already in the tree — no rustls/OpenSSL, so the static-musl /
/// distroless build is untouched (resolved decision #4).
type HttpClient = Client<HttpConnector, Full<Bytes>>;

/// The webhook delivery engine. Owns the config source ([`Db`]), the live-log
/// [`EventBus`] (for the synthetic delivery line), the outbound HTTP client, and
/// a process-monotonic `sequencer` counter. Cheap to clone (all handles).
#[derive(Clone)]
pub struct Notifier {
    db: Db,
    events: EventBus,
    /// Suppress the pretty stdout line for the synthetic delivery event.
    quiet: bool,
    client: HttpClient,
    /// Monotonic source of the payload `sequencer` field (a local counter, not
    /// AWS's value — documented as such).
    seq: Arc<AtomicU64>,
}

impl Notifier {
    /// Build a notifier over `db` (config) and `events` (the delivery line).
    pub fn new(db: Db, events: EventBus, quiet: bool) -> Self {
        let client = Client::builder(TokioExecutor::new()).build_http();
        Self {
            db,
            events,
            quiet,
            client,
            seq: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Fire notifications for `event` — **returns immediately**. Loading config,
    /// rendering, and POSTing all happen on a background task, so a slow,
    /// unreachable, or erroring receiver can never stall or fail the object
    /// mutation that triggered it (spec: "never stall an upload").
    pub fn notify(&self, event: ObjectEvent) {
        let this = self.clone();
        tokio::spawn(async move {
            this.deliver_all(event).await;
        });
    }

    /// Load the bucket's destinations, keep the ones matching this event, and
    /// deliver to each independently. Best-effort: a DB error drops silently
    /// (this is a dev tool, not a delivery bus).
    async fn deliver_all(&self, event: ObjectEvent) {
        let db = self.db.clone();
        let bucket = event.bucket.clone();
        let rows = match tokio::task::spawn_blocking(move || db.list_bucket_notifications(&bucket))
            .await
        {
            Ok(Ok(rows)) => rows,
            _ => return,
        };
        for row in rows {
            if destination_matches(
                &row.events,
                row.prefix.as_deref(),
                row.suffix.as_deref(),
                &event,
            ) {
                self.deliver_one(&row, &event).await;
            }
        }
    }

    /// Render and POST to one destination (exactly one attempt, bounded by its
    /// `timeout_ms`), then emit the synthetic live-log line naming the
    /// destination and outcome.
    async fn deliver_one(&self, row: &NotificationRow, event: &ObjectEvent) {
        let sequencer = format!("{:016x}", self.seq.fetch_add(1, Ordering::Relaxed));
        let event_time = iso8601_now();
        let id = gen_uuid();
        let ctx = PayloadCtx {
            region: "us-east-1",
            event_time: &event_time,
            sequencer: &sequencer,
            id: &id,
        };
        let payload = render_payload(&row.format, event, &ctx);
        let body = serde_json::to_vec(&payload).unwrap_or_default();

        let start = Instant::now();
        let outcome = self.post(&row.url, row.timeout_ms, body).await;
        let duration_ms = start.elapsed().as_millis() as u64;
        let status = match outcome {
            Outcome::Status(code) => code,
            _ => 0,
        };
        let note = format!("→ webhook {} {}", row.url, outcome.label());
        self.emit(event, status, duration_ms, note);
    }

    /// One POST attempt bounded by `timeout_ms`; no retry. A connect failure,
    /// timeout, or non-2xx is reported in the [`Outcome`] (logged, never
    /// surfaced to the S3 client).
    async fn post(&self, url: &str, timeout_ms: i64, body: Vec<u8>) -> Outcome {
        let req = match hyper::Request::builder()
            .method(hyper::Method::POST)
            .uri(url)
            .header(hyper::header::CONTENT_TYPE, "application/json")
            .body(Full::new(Bytes::from(body)))
        {
            Ok(req) => req,
            Err(e) => return Outcome::Error(format!("bad request: {e}")),
        };
        let bound = Duration::from_millis(timeout_ms.max(1) as u64);
        match tokio::time::timeout(bound, self.client.request(req)).await {
            Ok(Ok(resp)) => Outcome::Status(resp.status().as_u16()),
            Ok(Err(e)) => Outcome::Error(e.to_string()),
            Err(_) => Outcome::Timeout,
        }
    }

    /// Emit the synthetic delivery event onto the live-log bus (and stdout unless
    /// quiet). Carries a `note` so the log names the destination + outcome; this
    /// is the one event the notifier writes to the bus.
    fn emit(&self, event: &ObjectEvent, status: u16, duration_ms: u64, note: String) {
        let ev = self.events.emit(EventDraft {
            method: "POST".to_owned(),
            op: Some("Webhook".to_owned()),
            bucket: Some(event.bucket.clone()),
            key: Some(event.key.clone()),
            status,
            duration_ms,
            bytes_in: 0,
            bytes_out: 0,
            auth: Auth::Anonymous,
            error_code: None,
            note: Some(note),
        });
        if !self.quiet {
            println!("{}", crate::events::pretty(&ev));
        }
    }
}

/// The result of one delivery attempt.
enum Outcome {
    /// The receiver replied with this HTTP status.
    Status(u16),
    /// The attempt exceeded the destination's `timeout_ms`.
    Timeout,
    /// A connect/transport error (message).
    Error(String),
}

impl Outcome {
    /// The short label used in the synthetic live-log note.
    fn label(&self) -> String {
        match self {
            Outcome::Status(code) => code.to_string(),
            Outcome::Timeout => "timeout".to_owned(),
            Outcome::Error(e) => format!("error: {e}"),
        }
    }
}

/// The current time as an ISO-8601 (RFC 3339) UTC string for the payload's
/// `eventTime`/`time` field.
fn iso8601_now() -> String {
    use time::format_description::well_known::Rfc3339;
    time::OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_default()
}

/// A random UUID-v4 string for the EventBridge `id` field. cubby has no real
/// EventBridge id; this is a well-formed placeholder (documented as such).
fn gen_uuid() -> String {
    let mut b = [0u8; 16];
    // Best-effort randomness; a failure leaves zeros, still a valid-shaped id.
    let _ = getrandom::getrandom(&mut b);
    b[6] = (b[6] & 0x0f) | 0x40; // version 4
    b[8] = (b[8] & 0x3f) | 0x80; // variant
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> PayloadCtx<'static> {
        PayloadCtx {
            region: "us-east-1",
            event_time: "2026-07-17T18:22:05.123Z",
            sequencer: "0000000000000001",
            id: "11111111-1111-1111-1111-111111111111",
        }
    }

    #[test]
    fn s3_notification_created_has_records_shape_with_etag_and_encoded_key() {
        let ev = ObjectEvent::created(
            "uploads",
            "photos/my cat.jpg",
            EventKind::Put,
            24173,
            "d41d8cd98f00b204e9800998ecf8427e",
        );
        let v = s3_notification_payload(&ev, &ctx());
        let rec = &v["Records"][0];
        assert_eq!(rec["eventSource"], "aws:s3");
        assert_eq!(rec["eventName"], "ObjectCreated:Put"); // no s3: prefix
        assert_eq!(rec["awsRegion"], "us-east-1");
        assert_eq!(rec["eventTime"], "2026-07-17T18:22:05.123Z");
        let s3 = &rec["s3"];
        assert_eq!(s3["bucket"]["name"], "uploads");
        assert_eq!(s3["bucket"]["arn"], "arn:aws:s3:::uploads");
        let obj = &s3["object"];
        // Space is encoded as '+', '/' stays literal.
        assert_eq!(obj["key"], "photos/my+cat.jpg");
        assert_eq!(obj["size"], 24173);
        assert_eq!(obj["eTag"], "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(obj["sequencer"], "0000000000000001");
    }

    #[test]
    fn s3_notification_removed_omits_size_and_etag() {
        let ev = ObjectEvent::removed("uploads", "photos/cat.jpg");
        let v = s3_notification_payload(&ev, &ctx());
        let obj = &v["Records"][0]["s3"]["object"];
        assert_eq!(v["Records"][0]["eventName"], "ObjectRemoved:Delete");
        assert_eq!(obj["key"], "photos/cat.jpg");
        assert!(obj.get("size").is_none(), "removal omits size");
        assert!(obj.get("eTag").is_none(), "removal omits eTag");
        // The sequencer is still present.
        assert_eq!(obj["sequencer"], "0000000000000001");
    }

    #[test]
    fn eventbridge_created_has_source_detail_type_and_lowercase_etag() {
        let ev = ObjectEvent::created(
            "uploads",
            "photos/cat.jpg",
            EventKind::Put,
            24173,
            "d41d8cd98f00b204e9800998ecf8427e",
        );
        let v = eventbridge_payload(&ev, &ctx());
        assert_eq!(v["source"], "aws.s3");
        assert_eq!(v["detail-type"], "Object Created");
        assert_eq!(v["id"], "11111111-1111-1111-1111-111111111111");
        assert_eq!(v["resources"][0], "arn:aws:s3:::uploads");
        let detail = &v["detail"];
        assert_eq!(detail["bucket"]["name"], "uploads");
        assert_eq!(detail["reason"], "PutObject");
        let obj = &detail["object"];
        assert_eq!(obj["key"], "photos/cat.jpg");
        assert_eq!(obj["size"], 24173);
        // Lowercase `etag` here (vs `eTag` in the S3-notification shape).
        assert_eq!(obj["etag"], "d41d8cd98f00b204e9800998ecf8427e");
        assert!(obj.get("eTag").is_none());
    }

    #[test]
    fn eventbridge_removed_is_object_deleted_and_omits_size_etag() {
        let ev = ObjectEvent::removed("uploads", "photos/cat.jpg");
        let v = eventbridge_payload(&ev, &ctx());
        assert_eq!(v["detail-type"], "Object Deleted");
        assert_eq!(v["detail"]["reason"], "DeleteObject");
        let obj = &v["detail"]["object"];
        assert!(obj.get("size").is_none());
        assert!(obj.get("etag").is_none());
    }

    #[test]
    fn gen_uuid_is_well_formed_v4() {
        let u = gen_uuid();
        assert_eq!(u.len(), 36);
        let parts: Vec<&str> = u.split('-').collect();
        assert_eq!(
            parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
            [8, 4, 4, 4, 12]
        );
        // Version nibble is 4; variant nibble is one of 8/9/a/b.
        assert_eq!(&parts[2][0..1], "4");
        assert!(matches!(&parts[3][0..1], "8" | "9" | "a" | "b"));
        // Two calls differ (randomness).
        assert_ne!(u, gen_uuid());
    }

    #[test]
    fn render_payload_selects_by_format() {
        let ev = ObjectEvent::created("b", "k", EventKind::Put, 1, "e");
        // eventbridge → has `source`, no `Records`.
        let eb = render_payload("eventbridge", &ev, &ctx());
        assert_eq!(eb["source"], "aws.s3");
        assert!(eb.get("Records").is_none());
        // default / unknown → s3-notification `{"Records":[…]}`.
        let s3 = render_payload("s3-notification", &ev, &ctx());
        assert!(s3.get("Records").is_some());
        let dflt = render_payload("something-else", &ev, &ctx());
        assert!(dflt.get("Records").is_some());
    }

    #[test]
    fn event_name_drops_the_s3_prefix_but_qualified_keeps_it() {
        // The S3-notification `eventName` field has no `s3:`; the config filter
        // name does — the quirk the payloads must honor.
        assert_eq!(EventKind::Put.event_name(), "ObjectCreated:Put");
        assert_eq!(EventKind::Put.qualified_name(), "s3:ObjectCreated:Put");
        assert_eq!(
            EventKind::CompleteMultipartUpload.event_name(),
            "ObjectCreated:CompleteMultipartUpload"
        );
        assert_eq!(EventKind::Delete.event_name(), "ObjectRemoved:Delete");
    }

    #[test]
    fn families_and_detail_types_split_created_vs_removed() {
        for k in [
            EventKind::Put,
            EventKind::Copy,
            EventKind::CompleteMultipartUpload,
        ] {
            assert_eq!(k.family(), "s3:ObjectCreated:*");
            assert_eq!(k.detail_type(), "Object Created");
            assert!(k.is_created());
        }
        assert_eq!(EventKind::Delete.family(), "s3:ObjectRemoved:*");
        assert_eq!(EventKind::Delete.detail_type(), "Object Deleted");
        assert!(!EventKind::Delete.is_created());
    }

    #[test]
    fn reasons_name_the_specific_api() {
        assert_eq!(EventKind::Put.reason(), "PutObject");
        assert_eq!(EventKind::Copy.reason(), "CopyObject");
        assert_eq!(
            EventKind::CompleteMultipartUpload.reason(),
            "CompleteMultipartUpload"
        );
        assert_eq!(EventKind::Delete.reason(), "DeleteObject");
    }

    #[test]
    fn event_subscribed_matches_exact_or_wildcard_family() {
        let put = EventKind::Put;
        // Exact specific event.
        assert!(event_subscribed(&["s3:ObjectCreated:Put".into()], put));
        // Wildcard family.
        assert!(event_subscribed(&["s3:ObjectCreated:*".into()], put));
        // A different specific event, or the other family, does not match.
        assert!(!event_subscribed(&["s3:ObjectCreated:Copy".into()], put));
        assert!(!event_subscribed(&["s3:ObjectRemoved:*".into()], put));
        // The removal family gates the delete event, not creations.
        assert!(event_subscribed(
            &["s3:ObjectRemoved:*".into()],
            EventKind::Delete
        ));
        assert!(!event_subscribed(
            &["s3:ObjectCreated:*".into()],
            EventKind::Delete
        ));
    }

    #[test]
    fn key_filters_gate_on_prefix_and_suffix() {
        // No constraints → always passes.
        assert!(key_passes_filters("photos/cat.jpg", None, None));
        // Prefix only.
        assert!(key_passes_filters("photos/cat.jpg", Some("photos/"), None));
        assert!(!key_passes_filters("docs/readme.md", Some("photos/"), None));
        // Suffix only.
        assert!(key_passes_filters("photos/cat.jpg", None, Some(".jpg")));
        assert!(!key_passes_filters("photos/cat.png", None, Some(".jpg")));
        // Both must hold.
        assert!(key_passes_filters(
            "photos/cat.jpg",
            Some("photos/"),
            Some(".jpg")
        ));
        assert!(!key_passes_filters(
            "photos/cat.png",
            Some("photos/"),
            Some(".jpg")
        ));
    }

    #[test]
    fn destination_matches_requires_both_event_and_filters() {
        let ev = ObjectEvent::created("uploads", "photos/cat.jpg", EventKind::Put, 10, "e");
        // Event subscribed + key passes → matches.
        assert!(destination_matches(
            &["s3:ObjectCreated:*".into()],
            Some("photos/"),
            Some(".jpg"),
            &ev
        ));
        // Event subscribed but key filtered out → no match.
        assert!(!destination_matches(
            &["s3:ObjectCreated:*".into()],
            Some("invoices/"),
            None,
            &ev
        ));
        // Key passes but event not subscribed → no match.
        assert!(!destination_matches(
            &["s3:ObjectRemoved:*".into()],
            Some("photos/"),
            None,
            &ev
        ));
    }

    #[test]
    fn validate_rejects_non_http_urls() {
        assert!(validate_destination(
            "https://example.com/hook",
            &["s3:ObjectCreated:*".into()],
            "s3-notification",
            5000
        )
        .is_err());
        assert!(validate_destination(
            "ftp://x",
            &["s3:ObjectCreated:*".into()],
            "s3-notification",
            5000
        )
        .is_err());
        assert!(validate_destination(
            "http://localhost:3000/hook",
            &["s3:ObjectCreated:*".into()],
            "s3-notification",
            5000
        )
        .is_ok());
    }

    #[test]
    fn validate_rejects_unknown_event_format_and_bad_timeout() {
        let url = "http://localhost:3000/hook";
        // Unknown event token.
        assert!(
            validate_destination(url, &["s3:ObjectMutated:*".into()], "s3-notification", 5000)
                .is_err()
        );
        // Empty events.
        assert!(validate_destination(url, &[], "s3-notification", 5000).is_err());
        // Unknown format.
        assert!(
            validate_destination(url, &["s3:ObjectCreated:*".into()], "protobuf", 5000).is_err()
        );
        // Non-positive timeout.
        assert!(
            validate_destination(url, &["s3:ObjectCreated:*".into()], "eventbridge", 0).is_err()
        );
        assert!(
            validate_destination(url, &["s3:ObjectCreated:*".into()], "eventbridge", -1).is_err()
        );
        // A fully valid destination passes.
        assert!(
            validate_destination(url, &["s3:ObjectCreated:Put".into()], "eventbridge", 200).is_ok()
        );
    }
}
