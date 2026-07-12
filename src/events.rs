//! The in-process live-request event bus.
//!
//! One [`Event`] is recorded per S3 request by the logging wrapper in
//! [`crate::http`]. The bus is a `tokio::sync::broadcast` channel plus a
//! ~1,000-event ring buffer (per CONCEPT): new SSE subscribers replay the
//! buffer (optionally from a `Last-Event-ID`) and then stream live, while a
//! slow consumer that lags the channel gets a synthetic "dropped" marker rather
//! than stalling the server. Nothing is persisted — the log resets on restart,
//! which is correct for a dev tool.
//!
//! The same event feeds three consumers: the SSE/ndjson endpoint, and a pretty
//! stdout line (see [`pretty`]).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tokio::sync::broadcast;

/// How the request was authenticated. `ui` is reserved for a future decision to
/// surface UI-originated mutations; today those are not logged at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Auth {
    /// SigV4 `Authorization` header.
    Header,
    /// SigV4 query-string (presigned URL).
    Presigned,
    /// No credentials presented.
    Anonymous,
}

/// One captured S3 request. `op` is the **resolved** S3 operation (e.g.
/// `CreateMultipartUpload`), absent only when the request failed before the
/// operation was resolved (e.g. a bad-signature 403). `error_code` is the S3
/// error code on failures. No headers or signatures are ever captured.
#[derive(Clone, Debug, Serialize)]
pub struct Event {
    pub id: u64,
    /// Unix milliseconds when the event was recorded.
    pub ts: i64,
    pub method: String,
    pub op: Option<String>,
    pub bucket: Option<String>,
    pub key: Option<String>,
    pub status: u16,
    pub duration_ms: u64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub auth: Auth,
    pub error_code: Option<String>,
}

/// Everything the logging wrapper knows before the bus stamps an `id`/`ts`.
#[derive(Clone, Debug)]
pub struct EventDraft {
    pub method: String,
    pub op: Option<String>,
    pub bucket: Option<String>,
    pub key: Option<String>,
    pub status: u16,
    pub duration_ms: u64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub auth: Auth,
    pub error_code: Option<String>,
}

/// Default ring-buffer capacity (events retained for replay).
const RING_CAPACITY: usize = 1000;
/// Broadcast channel capacity: how far a live consumer may lag before it is
/// told events were dropped.
const CHANNEL_CAPACITY: usize = 1024;

struct Inner {
    tx: broadcast::Sender<Event>,
    ring: Mutex<VecDeque<Event>>,
    next_id: AtomicU64,
    capacity: usize,
}

/// Cheap-to-clone handle to the shared event bus.
#[derive(Clone)]
pub struct EventBus {
    inner: Arc<Inner>,
}

impl EventBus {
    /// A fresh bus with the default ring capacity.
    pub fn new() -> Self {
        Self::with_capacity(RING_CAPACITY)
    }

    fn with_capacity(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(CHANNEL_CAPACITY);
        Self {
            inner: Arc::new(Inner {
                tx,
                ring: Mutex::new(VecDeque::with_capacity(capacity.min(64))),
                next_id: AtomicU64::new(1),
                capacity,
            }),
        }
    }

    /// Stamp `draft` with a monotonic id and the current time, retain it in the
    /// ring buffer, broadcast it to live subscribers, and return the finished
    /// event (so the caller can also print a stdout line). Broadcasting under
    /// the ring lock keeps id order and makes [`EventBus::subscribe`] atomic:
    /// no event slips between a subscriber's backlog snapshot and its live feed.
    pub fn emit(&self, draft: EventDraft) -> Event {
        let event = Event {
            id: self.inner.next_id.fetch_add(1, Ordering::Relaxed),
            ts: now_millis(),
            method: draft.method,
            op: draft.op,
            bucket: draft.bucket,
            key: draft.key,
            status: draft.status,
            duration_ms: draft.duration_ms,
            bytes_in: draft.bytes_in,
            bytes_out: draft.bytes_out,
            auth: draft.auth,
            error_code: draft.error_code,
        };
        let mut ring = self.inner.ring.lock().expect("event ring poisoned");
        if ring.len() == self.inner.capacity {
            ring.pop_front();
        }
        ring.push_back(event.clone());
        // Ignore the "no live subscribers" error — the ring still retains it.
        let _ = self.inner.tx.send(event.clone());
        event
    }

    /// Subscribe for live events, returning the replay backlog first. `after`
    /// filters the backlog to events with `id > after` (the `Last-Event-ID`
    /// resume point); `None` replays the whole buffer.
    pub fn subscribe(&self, after: Option<u64>) -> (Vec<Event>, broadcast::Receiver<Event>) {
        let ring = self.inner.ring.lock().expect("event ring poisoned");
        let rx = self.inner.tx.subscribe();
        let backlog: Vec<Event> = ring
            .iter()
            .filter(|e| after.is_none_or(|a| e.id > a))
            .cloned()
            .collect();
        (backlog, rx)
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Current Unix time in milliseconds (saturating at the epoch).
fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Format an event as the compact, aligned stdout line
/// (`PUT  uploads/photos/cat.jpg   200   12ms   2.4MB`).
pub fn pretty(e: &Event) -> String {
    let target = match (&e.bucket, &e.key) {
        (Some(b), Some(k)) => format!("{b}/{k}"),
        (Some(b), None) => b.clone(),
        _ => "-".to_owned(),
    };
    format!(
        "{:<6} {:<40} {:>3} {:>7} {:>9}",
        e.method,
        target,
        e.status,
        format!("{}ms", e.duration_ms),
        human_bytes(e.bytes_in.max(e.bytes_out)),
    )
}

/// Human-readable byte size for the stdout line (e.g. `2.4MB`, `512B`).
fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if n < 1024 {
        return format!("{n}B");
    }
    let mut v = n as f64;
    let mut unit = 0;
    while v >= 1024.0 && unit < UNITS.len() - 1 {
        v /= 1024.0;
        unit += 1;
    }
    format!("{v:.1}{}", UNITS[unit])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(method: &str, op: Option<&str>) -> EventDraft {
        EventDraft {
            method: method.to_owned(),
            op: op.map(str::to_owned),
            bucket: Some("b".to_owned()),
            key: Some("k".to_owned()),
            status: 200,
            duration_ms: 1,
            bytes_in: 10,
            bytes_out: 0,
            auth: Auth::Header,
            error_code: None,
        }
    }

    #[test]
    fn ids_are_monotonic_from_one() {
        let bus = EventBus::new();
        let a = bus.emit(draft("PUT", Some("PutObject")));
        let b = bus.emit(draft("GET", Some("GetObject")));
        assert_eq!(a.id, 1);
        assert_eq!(b.id, 2);
        assert!(b.ts >= a.ts);
    }

    #[test]
    fn ring_evicts_oldest_beyond_capacity() {
        let bus = EventBus::with_capacity(3);
        for _ in 0..5 {
            bus.emit(draft("PUT", Some("PutObject")));
        }
        let (backlog, _rx) = bus.subscribe(None);
        // Only the last 3 survive; ids 3,4,5.
        assert_eq!(backlog.len(), 3);
        assert_eq!(backlog.first().unwrap().id, 3);
        assert_eq!(backlog.last().unwrap().id, 5);
    }

    #[test]
    fn subscribe_backlog_respects_after_id() {
        let bus = EventBus::new();
        for _ in 0..4 {
            bus.emit(draft("PUT", Some("PutObject")));
        }
        let (backlog, _rx) = bus.subscribe(Some(2));
        let ids: Vec<u64> = backlog.iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![3, 4]);
    }

    #[tokio::test]
    async fn live_subscribers_receive_events_emitted_after_subscribe() {
        let bus = EventBus::new();
        let (_backlog, mut rx) = bus.subscribe(None);
        let emitted = bus.emit(draft("GET", Some("GetObject")));
        let received = rx.recv().await.unwrap();
        assert_eq!(received.id, emitted.id);
        assert_eq!(received.op.as_deref(), Some("GetObject"));
    }

    #[test]
    fn pretty_line_is_aligned_and_human() {
        let e = Event {
            id: 1,
            ts: 0,
            method: "PUT".to_owned(),
            op: Some("PutObject".to_owned()),
            bucket: Some("uploads".to_owned()),
            key: Some("photos/cat.jpg".to_owned()),
            status: 200,
            duration_ms: 12,
            bytes_in: 2_516_582,
            bytes_out: 0,
            auth: Auth::Header,
            error_code: None,
        };
        let line = pretty(&e);
        assert!(line.starts_with("PUT   "), "line: {line:?}");
        assert!(line.contains("uploads/photos/cat.jpg"));
        assert!(line.contains("200"));
        assert!(line.contains("12ms"));
        assert!(line.contains("2.4MB"), "line: {line:?}");
    }

    #[test]
    fn human_bytes_scales_units() {
        assert_eq!(human_bytes(512), "512B");
        assert_eq!(human_bytes(2048), "2.0KB");
        assert_eq!(human_bytes(2_516_582), "2.4MB");
    }
}
