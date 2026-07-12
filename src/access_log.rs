//! Access hook that enriches the in-flight request-log event.
//!
//! Capturing a request is **two-part, joined through request extensions**: the
//! HTTP logging wrapper in [`crate::http`] is the authoritative emitter (it
//! always runs, times the request, and reads the final status/bytes), while
//! this [`S3Access`] impl runs *inside* `s3s` and enriches the event with the
//! **resolved operation name** (`cx.s3_op().name()`) and the auth kind — data
//! only `s3s` knows. The wrapper inserts a [`SharedSlot`] into the request
//! extensions; this hook fills it.
//!
//! It is an observer, not a new gate: it preserves `s3s`'s default policy
//! (a signature is required), so anonymous requests are still denied `403`
//! `AccessDenied` — but now *after* the op is resolved and recorded, so the log
//! shows `--no-sign-request` attempts with their real operation.

use std::sync::{Arc, Mutex};

use s3s::access::{S3Access, S3AccessContext};
use s3s::{s3_error, S3Result};

use crate::events::Auth;

/// Fields the access hook contributes to a request's event, shared with the
/// HTTP wrapper via request extensions.
#[derive(Default)]
pub struct CaptureSlot {
    /// The resolved S3 operation name (e.g. `CreateMultipartUpload`).
    pub op: Option<String>,
    /// How the request authenticated.
    pub auth: Option<Auth>,
}

/// A capture slot shared between the HTTP wrapper (which creates it) and the
/// access hook (which fills it).
pub type SharedSlot = Arc<Mutex<CaptureSlot>>;

/// The observer access hook. Records op + auth, then applies `s3s`'s default
/// signature-required policy.
pub struct AccessLog;

#[async_trait::async_trait]
impl S3Access for AccessLog {
    async fn check(&self, cx: &mut S3AccessContext<'_>) -> S3Result<()> {
        // Classify auth before taking the mutable extensions borrow.
        let has_creds = cx.credentials().is_some();
        let presigned = cx
            .uri()
            .query()
            .is_some_and(|q| q.contains("X-Amz-Signature"));
        let auth = match (has_creds, presigned) {
            (true, true) => Auth::Presigned,
            (true, false) => Auth::Header,
            (false, _) => Auth::Anonymous,
        };
        let op = cx.s3_op().name().to_owned();

        if let Some(slot) = cx.extensions_mut().get::<SharedSlot>() {
            let mut slot = slot.lock().expect("capture slot poisoned");
            slot.op = Some(op);
            slot.auth = Some(auth);
        }

        // Preserve the default gate: a valid signature is required. (Anonymous
        // requests were still recorded above, so the log explains the 403.)
        if has_creds {
            Ok(())
        } else {
            Err(s3_error!(AccessDenied, "Signature is required"))
        }
    }
}
