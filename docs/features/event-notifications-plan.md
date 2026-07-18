# Event notifications via webhook — plan

**Status:** done — all Steps + Acceptance boxes checked · **Spec:** [event-notifications-spec.md](event-notifications-spec.md) · **Roadmap:** v0.2 (Browser-facing & workflow)

## Approach

A new `Notifier` component owns the whole webhook path: it loads a bucket's
destinations from SQLite, filters by event/prefix/suffix, renders the payload in
the requested format, and POSTs it on a **background task** so delivery never
blocks or fails an object mutation (*dev-tool first* + "never stall an upload").
It reuses infrastructure already in the tree — `Db` for config, the existing
`EventBus` for the synthetic delivery line, and a thin `hyper_util` legacy client
(http-only, plus `tokio::time::timeout`) so cubby's **first outbound HTTP**
capability adds **no TLS stack and no new dependency** (resolved decision #4,
keeping the static-musl/distroless build untouched).

Config is **mutable bucket state in SQLite** (a new `bucket_notifications` table,
`ON DELETE CASCADE` from `buckets`), created/removed at runtime through a thin
`/_/api/buckets/{bucket}/notifications` seam with **write-time validation** —
mirroring how AWS models notification config as an API-set bucket property, not a
boot-time file (so startup is unchanged; an empty table is dormant).

The one architectural wrinkle: the codebase has **two** object-mutation paths —
the `Store` S3 methods and the `api/objects.rs` UI handlers (which write directly
against `Db`, not through `Store`). The spec's "fires at the store layer
regardless of origin" is therefore realized by invoking the same `Notifier` at
**both** sets of sites. A `Store::with_notifier` builder attaches the notifier
only to the router-built store, so the seed-path store (and tests) fire nothing —
which is exactly how "seed writes don't fire" falls out for free.

## Files

- `src/notify.rs` — **new.** `Notifier` (holds `Db`, `EventBus`, the outbound
  hyper client, a monotonic sequencer counter); `ObjectEvent` (bucket, key,
  `EventKind`, optional size/etag); `EventKind` (`Put`/`Copy`/`CompleteMultipartUpload`/`Delete`)
  with its `eventName`/family/reason/`detail-type` mappings; pure
  match/filter/validate helpers; the two payload builders; `notify()` that spawns
  the background delivery. Unit-tested for the pure parts.
- `src/db.rs` — add the `bucket_notifications` table to `SCHEMA_V0` (additive,
  `IF NOT EXISTS`, FK `ON DELETE CASCADE`); `NotificationRow` struct + `insert_`/
  `list_`/`delete_bucket_notification`. Change `delete_object` → `Result<bool>`
  (row removed?) and `delete_objects` → `Result<Vec<String>>` (keys actually
  removed) so notifications fire only for real removals.
- `src/api/notifications.rs` — **new.** `list`/`create`/`delete` handlers for the
  seam; JSON shapes; validation → `400`.
- `src/api/mod.rs` — route `buckets/{bucket}/notifications[/{id}]` (before the
  objects fallback, since both start `buckets/`).
- `src/store.rs` — `Store::with_notifier(self, Notifier) -> Self`; fire an
  `ObjectEvent` after each successful mutation (`put_object`, `copy_object`,
  `complete_multipart_upload`, `delete_object`, `delete_objects`). Adjust the two
  changed `Db` delete return types.
- `src/api/objects.rs` — fire in the UI `upload` and `delete` handlers via
  `state.notifier`; adjust the `delete_object` return type.
- `src/http.rs` — build the `Notifier` in `build_router` (from `cfg.db` +
  `cfg.events`), attach it to the store via `with_notifier`, and add it to
  `AppState` for the UI object path.
- `src/events.rs` — add an optional `note: Option<String>` to `Event`/
  `EventDraft` (default `None`) so a webhook delivery line can name its
  destination; `pretty()` appends it when present. Real S3 events keep `note:
  None`, unchanged.
- `examples/webhook_sink.rs` — **new.** A committed Cargo example receiver
  (`cargo run --example webhook_sink -- --port 3000`): a small `hyper`/`hyper-util`
  server (no new deps) that prints each received POST — method, path, and
  pretty-printed JSON body, plus a one-line `parsed as S3 event ✓` note when an
  `s3-notification` body deserializes. Flags exercise cubby's delivery
  semantics: `--delay <ms>` (sleep before replying, to trip a destination's
  `timeout_ms`) and `--status <code>` (reply non-2xx, to prove log-and-drop with
  no retry). This is the human/e2e + docs receiver; automated acceptance tests
  keep their own in-process capture receiver.
- `web/src/…` — a per-bucket **Notifications panel** in the bucket-browser route
  + `api.ts` client methods; regenerate `web/dist/` with `zero build`.
- `README.md` — document the feature, the two payload formats, per-bucket/
  per-path config, the integration recipes, and the http-only/sequencer/
  placeholder-fields caveats.
- `tests/…` — a local HTTP receiver harness (a tiny in-test hyper server that
  records requests) + acceptance tests driving AWS CLI/boto3 and the seam.

## Risks & unknowns

- **Two write paths.** Firing must be added at both `Store` and
  `api/objects.rs`; a future third mutation path would need the same hookup.
  Mitigated by funneling every site through one `Notifier::notify` call and a
  shared `ObjectEvent`.
- **size/eTag at delete-idempotency.** `ObjectRemoved` omits size/etag, and we
  fire only when a row was actually removed — hence the `delete_object`/
  `delete_objects` return-type change. Created events pass size/etag from the
  call site (already in hand) to avoid a re-query race.
- **`web/dist/` regeneration.** Per README the UI is a committed artifact built
  with `zero`; there is no CI freshness gate, so `zero build` must be run and the
  regenerated `web/dist/` committed with the UI step. Backend steps are fully
  testable via curl + the receiver *without* the UI, so the UI lands late.
- **Event-struct change.** Adding `note` is additive (`Option`, defaults `None`);
  the live-log UI renderer may optionally surface it, but ndjson/SSE serialize it
  for free, so acceptance doesn't depend on a UI change.
- **Additive schema on existing dirs.** The new table is created via `IF NOT
  EXISTS` on open; FK cascade relies on `PRAGMA foreign_keys=ON` (already set in
  `Db::open`).

## Steps

Each box ≈ one small commit moving an observable behavior. Check only when the
outcome is real, driven by the named observer.

- [x] **Schema + config CRUD.** Add `bucket_notifications` (FK → `buckets(name)`
      `ON DELETE CASCADE`) and `insert`/`list`/`delete_bucket_notification`. Unit
      tests: insert→list round-trips a row; delete removes it; deleting the
      parent bucket cascades its rows away.
- [x] **Delete methods report removals.** `delete_object` → `bool`,
      `delete_objects` → `Vec<String>` (keys actually removed); update all
      callers. Existing S3/UI delete behavior and responses unchanged (unit +
      existing tests green).
- [x] **`notify` pure core.** `EventKind` mappings (`ObjectCreated:Put`, family
      `s3:ObjectCreated:*`, reason `PutObject`, detail-type `Object Created`, …),
      event-list matching (wildcard family or exact), prefix/suffix filtering,
      and destination validation (http-only url, known format, known events,
      `timeout_ms > 0`). Unit tests cover each rule.
- [x] **Payload builders match AWS shapes.** `s3-notification` (`{"Records":[…]}`,
      `eventName` **without** `s3:`, `eTag`, url-encoded key, `sequencer`) and
      `eventbridge` (`source:"aws.s3"`, `detail-type`, lowercase `etag`,
      `detail.reason`, generated uuid `id`). Unit tests assert the exact JSON
      field names/shapes for a created and a removed event.
- [x] **Config seam.** `GET/POST/DELETE /_/api/buckets/{bucket}/notifications`
      wired in `api/mod.rs`. `curl` POST of a valid destination → `201` with an
      `id`; GET lists it; DELETE removes it. Invalid destination (unknown event,
      bad `format`, non-`http://` url, `timeout_ms<=0`) → `400`, nothing
      persisted.
- [x] **Delivery engine.** `Notifier::notify` spawns a background task: load
      matching destinations, render, POST via the hyper client bounded by
      `timeout_ms` (one attempt, no retry), emit a synthetic `note`-carrying
      live-log event. Driven through the seam + Store (next step) but provable
      here with a unit/integration test against the in-test receiver: a dead port
      and a sleeping receiver both return control immediately and log a failure.
- [x] **Fire from the S3 store path.** `Store::with_notifier` + fire after
      success in `put_object`/`copy_object`/`complete_multipart_upload`/
      `delete_object`/`delete_objects`; build+attach the notifier in
      `build_router`. Observer: with a destination configured, `aws s3 cp`,
      `aws s3 rm`, a boto3 >8MB multipart, `aws s3 cp` (copy), and a batch delete
      each deliver the expected event(s) to the receiver.
- [x] **Fire from the UI object path.** Fire in `api/objects.rs` `upload`/
      `delete` via `state.notifier`. Observer: a UI `PUT`/`DELETE` through
      `/_/api/buckets/…/objects/…` delivers the webhook, while the live-log
      stream still shows nothing for it (the intended divergence).
- [x] **Filtering & fan-out end-to-end.** Prefix, suffix, event, per-path
      routing to distinct URLs, and overlapping-filter fan-out all behave per
      spec, verified with two/more receivers and the seam.
- [x] **Example webhook sink.** `examples/webhook_sink.rs` prints each received
      POST (method/path/pretty JSON, with `parsed as S3 event ✓` on
      `s3-notification` bodies) and honors `--delay`/`--status`. Observer:
      `cargo run --example webhook_sink -- --port 3000`, then an `aws s3 cp`
      against a matched destination prints the event; `--delay 2000` with a
      `timeout_ms:500` destination shows cubby logging a timeout while the upload
      still returns. This is the manual e2e + UI-acceptance driver.
- [x] **UI Notifications panel.** Add the per-bucket panel (list destinations +
      add form with url/event checkboxes/prefix/suffix/format + per-row delete)
      to the bucket-browser route and the `api.ts` calls; run `zero build` and
      commit `web/dist/`. Observer: a human adds a destination in the browser and
      a subsequent `aws s3 cp` fires it — no restart.
- [x] **Docs** — update `README.md` for the notifications feature (config surface,
      both payload formats, per-bucket/per-path filtering + fan-out divergence,
      the integration recipes pointing at `examples/webhook_sink.rs` as the
      runnable receiver, and the http-only / local-monotonic-`sequencer` /
      placeholder-fields caveats).

## Acceptance

Mirrors the spec. `/implement` isn't done until every box passes by driving the
named observer (a local HTTP receiver captures POSTs; AWS CLI/boto3 generate
mutations; `curl` drives the seam; a human drives the UI; live-log ndjson and the
filesystem/SQLite cross-check).

### Firing & payload
- [x] **PutObject → `ObjectCreated:Put`.** `aws s3 cp` to a matched key delivers
      one `s3-notification` POST with `eventName:"ObjectCreated:Put"`,
      `s3.bucket.name`, `s3.object.key`, and `size`/`eTag` equal to
      `head-object`.
- [x] **DeleteObject → `ObjectRemoved:Delete`.** `aws s3 rm` delivers one POST
      with `eventName:"ObjectRemoved:Delete"` and no `size`/`eTag`.
- [x] **Multipart completion → one `CompleteMultipartUpload`.** A boto3 >8MB
      `upload_file` delivers exactly one `ObjectCreated:CompleteMultipartUpload`
      whose `eTag` is the `-N` composite.
- [x] **CopyObject → `ObjectCreated:Copy`.** `aws s3 cp s3://…/a s3://…/b`
      delivers `ObjectCreated:Copy` for `b`.
- [x] **DeleteObjects → one event per removed key.** A batch delete of three
      existing keys delivers three `ObjectRemoved:Delete` events.
- [x] **UI mutations fire (diverges from the live log).** A UI upload delivers
      `ObjectCreated:Put` and a UI delete delivers `ObjectRemoved:Delete`, though
      neither appears in the live-log stream.

### Format selector
- [x] **`format: eventbridge`** yields `source:"aws.s3"` / `detail-type:"Object
      Created"` / `detail.object.key` (lowercase `etag`) for the same PUT that an
      `s3-notification` destination renders as `{"Records":[…]}`.
- [x] **Default format is `s3-notification`** when `format` is omitted.

### Filtering
- [x] **Prefix filter** gates delivery (`photos/` fires, `docs/` doesn't).
- [x] **Suffix filter** gates delivery (`.jpg` fires, `.png` doesn't).
- [x] **Event filter** gates delivery (`s3:ObjectCreated:*` fires on PUT, not on
      the later DELETE).
- [x] **Per-path routing** — `photos/`→A, `invoices/`→B, each isolated.
- [x] **Overlapping filters fan out** — two matching destinations both receive
      the PUT (cubby diverges from AWS's rejection).

### Delivery semantics
- [x] **Dead receiver never blocks** — `aws s3 cp` returns promptly against a
      closed/sleeping receiver; the object is on disk; the failure is logged.
- [x] **`timeout_ms` honored per destination** — `timeout_ms:500` times out on a
      ~2s receiver while `timeout_ms:5000` succeeds; omitting it applies 5000.
- [x] **Delivery visible in the live log** — a synthetic webhook line naming the
      destination and status appears in `/_/api/events?format=ndjson`.
- [x] **No config, no change** — a bucket with no destinations never POSTs;
      startup unchanged.

### Config management
- [x] **Add via seam persists** — POST → `201`+`id`; GET lists it; row present.
- [x] **Add in UI fires, no restart** — human adds a destination, next `aws s3
      cp` fires it.
- [x] **Delete stops firing** — DELETE (or UI row delete) removes it; next
      matching PUT delivers nothing.
- [x] **Invalid rejected at write time** — bad event/format/url/`timeout_ms` →
      `400`, nothing persisted.
- [x] **Survives restart / travels with the dir** — after restart on the same
      data dir the destination is still listed and still fires.
- [x] **Bucket delete cascades** — after `aws s3 rb`, no `bucket_notifications`
      rows remain for that bucket.

### Fidelity
- [x] **Payload parses as a real AWS event** — the `s3-notification` body
      deserializes via a real SDK's S3-event type (e.g. Go `events.S3Event`, or
      boto3-style `body["Records"][0]["s3"]["object"]["key"]`) with the expected
      key/size.

## Progress notes

- **`bucket_notifications.created_at` is `INTEGER` (Unix seconds), not the spec
  sketch's `TEXT`.** Chosen for consistency with every other `created_at` in the
  schema (`buckets`, etc.) and the existing `iso8601()` seam helper, which renders
  it to an ISO string in the JSON. No behavioral difference.
- **Multipart acceptance was driven via `aws-sdk-s3` manual multipart, not boto3
  `upload_file`.** boto3 is not installed in this environment. The observable
  outcome the box names — exactly one `ObjectCreated:CompleteMultipartUpload` with
  the `-N` composite ETag — is asserted end-to-end in
  `tests/notifications.rs::s3_multipart_complete_fires_once_with_composite_etag`;
  the store fires the same event regardless of which SDK assembled the parts. The
  fidelity box's boto3-style dict access (`body["Records"][0]["s3"]["object"]
  ["key"]`) is exercised directly by the integration tests and `webhook_sink`.
- **Firing lives at the S3 trait handlers + the two UI object handlers**, both
  funneling through one `Store::fire`/`Notifier::notify`. `put_bytes` (shared with
  seeding) does **not** fire — the PutObject handler fires after it returns — so
  seed writes stay silent without relying on the notifier being absent.
- **`Notifier` shared by both write paths.** `build_router` builds one `Notifier`
  and hands it to both the store (`with_notifier`) and `AppState` (for the UI
  object path), so a mutation fires regardless of origin.
