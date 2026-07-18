# Event notifications via webhook — spec

**Status:** done — shipped (see `event-notifications-plan.md`) · **Roadmap:** v0.2
("Browser-facing & workflow"; CONCEPT names event notifications the *first
post-MVP promotion*) · **Slug:** `event-notifications`

## Why

Event-driven S3 is one of the most common shapes a real app takes: an object
lands → a thumbnail is generated, an import job kicks off, a row is cleaned up.
In production the object event flows S3 → SNS / SQS / Lambda / EventBridge → the
app's code. To develop or debug that flow on a laptop today you either hand-mock
the event or pay for **LocalStack Pro**, which gates S3 notifications behind its
paid tier; MinIO (which had webhook notifications) has gone closed. There is no
free, MIT, zero-config way to exercise "my app reacts to an S3 event" locally.

cubby can be exactly that. It already records every mutation and already has an
in-process event bus feeding the live log. A webhook sink is a small, on-thesis
addition: **cubby POSTs a JSON event to a URL your app exposes when an object is
created or removed**, shaped byte-for-byte like the event AWS would deliver — so
the handler you write against cubby runs unchanged in prod.

North stars served:
- **Dev tool first.** This is the headline: test your app's reaction to S3
  events end-to-end, locally, against real object bytes on disk — the thing
  LocalStack paywalls.
- **Starts in milliseconds, zero config.** Opt-in only. With no notification
  config, behavior is exactly as today; nothing fires, nothing blocks.
- **Compatibility is proven, not claimed.** The event payload matches AWS's two
  real shapes (S3-notification and EventBridge), verified by a receiver that
  captures the POST — not by asserting we "support notifications."

## The fidelity model (what cubby does and does not stand in for)

Every S3 event-driven flow has two hops:

1. **S3 → routing service** (the *event-source* edge): S3 emits into SNS / SQS /
   Lambda / EventBridge.
2. **routing service → your code** (the *delivery* edge): Lambda invokes, SQS is
   polled, SNS pushes, EventBridge matches a rule and hits a target.

cubby models **edge #1 only**, and replaces the whole AWS routing layer with the
one transport that needs no AWS: an HTTP POST. The developer supplies a small
local endpoint that stands in for edge #2. The fidelity cubby preserves is the
**event payload shape** — the part the app's business logic actually consumes —
not the transport semantics of any particular routing service.

This is why the **payload format is selectable per destination**: AWS does not
emit one universal shape. Notifications to SQS/SNS/Lambda use the
**S3-notification** format (`{"Records":[{"s3":{…}}]}`); the S3 → EventBridge
path uses the **EventBridge** format (`{"source":"aws.s3","detail-type":"Object
Created","detail":{…}}`). A developer whose prod target is EventBridge writes
`event["detail"]["object"]["key"]`; one whose target is a Lambda-direct
integration writes `event["Records"][0]["s3"]["object"]["key"]`. Same fact,
different schema. The `format` selector lets each developer's handler see the
JSON its prod target sees.

## In scope

- **A per-bucket notification config, stored in SQLite** (`meta.sqlite`, a new
  table) as first-class mutable bucket state — not a startup file. Each bucket
  owns zero or more destinations; each destination has: a `url` (an **`http://`**
  endpoint cubby POSTs to — see below; `https://` is out of scope for v0.2), an
  `events` list (which object-lifecycle events fire it), an optional `prefix`
  and/or `suffix` key filter, a `format` (`s3-notification` | `eventbridge`,
  default `s3-notification`), and an optional **`timeout_ms`** (per-destination
  delivery timeout; default **5000**) so a slow receiver — e.g. a container that
  cold-starts — can be given more headroom than a fast in-process one.
- **Managed at runtime through the Web UI + a thin `/_/api/` seam** — the same
  read/write JSON pattern the rest of the UI uses. A developer views a bucket's
  destinations, adds one, and deletes one from the browser; **changes take effect
  immediately, no restart** (cubby reads the config from SQLite on each mutation
  or keeps an in-memory view it refreshes on write). This mirrors how AWS
  actually works — notification config is mutable bucket state set through an
  API, not a static file — and gives a tight dev loop: add a webhook, trigger an
  upload, watch it fire.
- **Object-lifecycle events**, resolved from the storage-mutation points (not
  the raw wire interceptor — see *Behavior*):
  - `s3:ObjectCreated:Put` — a successful `PutObject`.
  - `s3:ObjectCreated:Copy` — a successful `CopyObject`.
  - `s3:ObjectCreated:CompleteMultipartUpload` — a completed multipart object.
  - `s3:ObjectRemoved:Delete` — a successful `DeleteObject`, and one event per
    deleted key in a `DeleteObjects` batch.
  - Config may subscribe to a wildcard family (`s3:ObjectCreated:*`,
    `s3:ObjectRemoved:*`) or a specific event.
- **Two payload formats**, matching AWS field-for-field where cubby has the data
  (see *Behavior* for the exact JSON): the S3-notification `{"Records":[…]}`
  envelope, and the EventBridge `{source, detail-type, detail}` envelope.
- **Async, best-effort delivery.** Firing happens *after* the object row is
  committed and **never blocks the client's response** or the write path. A
  slow, unreachable, or 500-returning receiver cannot stall an upload.
- **Live-log visibility.** Each delivery attempt surfaces as a synthetic line in
  the existing live log / stdout stream (e.g. `→ webhook uploads/photos/cat.jpg
  200 8ms`), so "watch the object land *and* watch cubby notify my app" is one
  stream — closing the S3-debugger story.

## Out of scope

- **The real `PutBucketNotificationConfiguration` / `GetBucketNotificationConfiguration`
  S3 API.** AWS's config model is ARN-based (its targets are SNS/SQS/Lambda
  ARNs); it has **no webhook target type**, so a URL cannot be expressed in it.
  cubby therefore keeps its own config surface (SQLite-backed, managed via the
  `/_/api/` seam), *not* that S3 verb — it reproduces the *pattern* (mutable
  bucket state set through an API) without the incompatible schema. (If a client
  someday *requires* those verbs to exist, revisit — but they can't carry a
  webhook URL regardless.)
- **A rules engine.** No EventBridge-style pattern matching, multiple buses,
  fan-out to N rules, archive/replay, or cross-account. Config is a flat list of
  "bucket + event + optional prefix/suffix → url". If a prod flow fans one event
  to five rules, you list five destinations.
- **Reproducing SQS/SNS/Lambda transport semantics** — no polling, visibility
  timeout, batching windows, SNS subscription-confirmation handshake, Lambda
  invocation/retry/DLQ model, or ordering guarantees beyond "fired after
  commit". cubby delivers the *event*; the developer's local shim bridges HTTP
  to however their code runs.
- **Signed/authenticated webhooks** (e.g. an HMAC signature header à la Stripe,
  or SNS message signatures). v0.2 is plain POST to a localhost dev endpoint. A
  shared-secret header is a deferred fast-follow (resolved decision 5).
- **At-least-once durability / a persistent outbox, and retries.** Best-effort,
  in-memory, like the live log: exactly one delivery attempt (resolved decision
  3). If cubby or the receiver is down, that event is lost. This is a dev tool,
  not a delivery bus.
- **Notifications on seed writes.** Objects created by `--seed` at startup
  (before the port binds) do **not** fire webhooks — seeding is fixture state,
  not a runtime event, and a boot-time webhook flood would surprise. (Open
  question: UI-originated uploads — see below.)
- **Versioning-derived events** (`ObjectRemoved:DeleteMarkerCreated`,
  `s3:ObjectRestore:*`, replication/lifecycle events) — versioning and lifecycle
  are CONCEPT non-goals.

## Behavior

### How config is stored & managed

Notification config is **mutable bucket state in SQLite**, created and removed at
runtime through the UI and its JSON seam — there is no config file and no
restart.

**Storage — a new `meta.sqlite` table (sketch; exact columns settled in `/plan`):**

```
bucket_notifications(
  id           INTEGER PK,        -- stable id, used in the DELETE path
  bucket       TEXT NOT NULL,     -- FK → buckets(name), ON DELETE CASCADE
  url          TEXT NOT NULL,
  events       TEXT NOT NULL,     -- the subscribed event list (JSON array)
  prefix       TEXT,              -- optional key-prefix filter
  suffix       TEXT,              -- optional key-suffix filter
  format       TEXT NOT NULL,     -- 's3-notification' | 'eventbridge'
  timeout_ms   INTEGER NOT NULL,  -- per-destination delivery timeout (default 5000)
  created_at   TEXT NOT NULL
)
```

Deleting a bucket cascades its destinations away (they're bucket state). The
config travels with the data directory like all other metadata — copy the dir,
copy the webhooks (see the open question on repo/CI portability).

**Seam — a few endpoints under the existing `/_/api/` (shapes settled in `/plan`):**

- `GET /_/api/buckets/{bucket}/notifications` → the bucket's destinations,
  `{"notifications":[{"id","url","events":[…],"prefix","suffix","format","created_at"}, …]}`.
- `POST /_/api/buckets/{bucket}/notifications` with a destination body →
  validates and inserts, returning the created row (with its `id`). **Validation
  happens here, at write time** (see below), not at startup.
- `DELETE /_/api/buckets/{bucket}/notifications/{id}` → removes one destination.

A single destination's logical shape (the POST body, and one row of the GET):

```json
{
  "url": "http://localhost:3000/s3-hook",
  "events": ["s3:ObjectCreated:*", "s3:ObjectRemoved:*"],
  "prefix": "photos/",
  "suffix": ".jpg",
  "format": "s3-notification",
  "timeout_ms": 5000
}
```

**UI — a per-bucket "Notifications" panel in the bucket browser.** Because config
is per-bucket, its natural home is the bucket view (not a new top-level nav
item): when a bucket is selected, a panel lists its destinations and offers an
**Add** form (url + event checkboxes + optional prefix/suffix + format) and a
per-row **delete**. (Exact placement is an open question below.)

**Validation at write time (not startup).** Because the config is edited live
rather than parsed from a file at boot, a bad destination is rejected **when you
try to save it**: `POST …/notifications` with an unknown event name, an invalid
`format`, a non-`http://` `url`, or a non-positive `timeout_ms` returns `400`
with a naming error and persists nothing. There is no "fail fast before bind"
step — startup is unchanged; an empty table means the feature is simply dormant.

**Matching rules (unchanged by where config lives):**
- A destination fires only when the event **matches** its `events` list **and**
  passes both `prefix` and `suffix` (each optional; absent = no constraint),
  exactly as S3's `FilterRule` prefix/suffix behave.
- Multiple destinations on one bucket each get their own independent POST.

### Configuration granularity — per-bucket and per-path (matching AWS)

This mirrors exactly how AWS models it:

- **Per-bucket is the native unit.** AWS attaches its notification configuration
  to a bucket; there is no first-class "notify on a path". cubby's destinations
  are rows keyed by bucket for the same reason.
- **Per-path is expressed as prefix/suffix filters,** not a separate config
  level. AWS's only allowed `FilterRule` names are `prefix` and `suffix` (no
  regex/globs) — which is exactly the `prefix`/`suffix` this spec puts on each
  destination. So "send `photos/` events to URL A and `invoices/` events to URL
  B" is two filtered destinations under the one bucket, each routing its path to
  its own URL. That is the full extent of per-path routing S3 itself offers.
- **Overlapping filters — a deliberate divergence from AWS.** AWS *rejects* a
  config in which two rules have overlapping prefixes for the same event type
  (it refuses ambiguous routing). cubby instead **allows overlap and fires every
  matching destination independently**: firing one object event at two local
  receivers (fan-out) is a legitimate thing a developer wants to *test*, and a
  dev tool shouldn't forbid it. This is the one place cubby is intentionally more
  permissive than S3; documented as such.

### Event set & resolution

Events are resolved at the **storage-mutation points** in the store layer
(`PutObject` success, `CopyObject` success, `CompleteMultipartUpload`,
`DeleteObject`, `DeleteObjects`), **not** from the live-log wire interceptor. The
wire event lacks the committed object's post-state (final `size`/`eTag`) and
includes reads and failures; a notification must carry the object's committed
size and ETag and must fire only on a successful mutation. `DeleteObjects` fires
**one event per key** actually removed.

Hooking the **store** (not the S3 request handler) has a deliberate consequence:
a mutation made through the **Web UI** (`/_/api/…` upload/delete) travels the
same store write/delete path, so it fires notifications too — see *Interaction
with existing surfaces*. Only successful commits fire; a mutation that errors
before committing does not.

Note the S3 quirk the payloads must honor: the config filter names events with
the `s3:` prefix (`s3:ObjectCreated:Put`), but the **S3-notification payload**'s
`eventName` field drops it (`ObjectCreated:Put`).

### Payload — `s3-notification` format

`Content-Type: application/json`, body:

```json
{
  "Records": [
    {
      "eventVersion": "2.1",
      "eventSource": "aws:s3",
      "awsRegion": "us-east-1",
      "eventTime": "2026-07-17T18:22:05.123Z",
      "eventName": "ObjectCreated:Put",
      "s3": {
        "s3SchemaVersion": "1.0",
        "configurationId": "cubby",
        "bucket": { "name": "uploads", "arn": "arn:aws:s3:::uploads" },
        "object": {
          "key": "photos/cat.jpg",
          "size": 24173,
          "eTag": "d41d8cd98f00b204e9800998ecf8427e",
          "sequencer": "0000000000000001"
        }
      }
    }
  ]
}
```

- `key` is URL-encoded exactly as S3 encodes it in this field (spaces as `+`,
  etc.) — matching AWS so handlers that decode it keep working.
- `size`/`eTag` are the committed object's values (a multipart object carries its
  `-N` composite ETag; `ObjectRemoved` events omit `size`/`eTag`).
- Fields cubby has no real value for (`userIdentity`, `requestParameters`,
  `responseElements`, `ownerIdentity`, real account id) are filled with stable
  dev placeholders or omitted — documented, so nobody expects a real principal.

### Payload — `eventbridge` format

```json
{
  "version": "0",
  "id": "<generated-uuid>",
  "detail-type": "Object Created",
  "source": "aws.s3",
  "account": "000000000000",
  "time": "2026-07-17T18:22:05Z",
  "region": "us-east-1",
  "resources": ["arn:aws:s3:::uploads"],
  "detail": {
    "version": "0",
    "bucket": { "name": "uploads" },
    "object": {
      "key": "photos/cat.jpg",
      "size": 24173,
      "etag": "d41d8cd98f00b204e9800998ecf8427e",
      "sequencer": "0000000000000001"
    },
    "reason": "PutObject"
  }
}
```

- `detail-type` is `"Object Created"` / `"Object Deleted"` (EventBridge's coarse
  types), and `detail.reason` carries the specific API (`PutObject`,
  `CopyObject`, `CompleteMultipartUpload`, `DeleteObject`). Note the field is
  lowercase `etag` here vs. `eTag` in the S3-notification shape — an AWS
  inconsistency cubby reproduces so handlers port cleanly.

### Delivery semantics

- Firing is **decoupled from the request**: the client's PUT/DELETE returns as
  soon as the object is committed; the POST happens on a background task. A
  receiver that hangs or errors never affects the S3 response.
- **Best-effort, exactly one attempt — no retry** (resolved). A single POST
  bounded by the destination's `timeout_ms` (**default 5000**, per-destination
  overridable); a connect failure, timeout, or non-2xx response is **logged and
  dropped**, never retried and never surfaced to the S3 client. Slow targets that
  cold-start (e.g. SAM Local in Docker) can raise their own `timeout_ms` so a
  legitimate slow invoke isn't logged as a spurious timeout. (Durable
  at-least-once delivery stays a non-goal — this is a dev tool.)
- **Ordering** is not guaranteed beyond "fired after commit"; concurrent
  mutations may deliver in any order (like S3, which is explicit that
  notifications aren't ordered without `sequencer` reasoning).
- Each attempt emits a **synthetic live-log/stdout line** with the destination,
  the resolved event, and the response status (or a failure marker), so
  deliveries are observable in the same stream as S3 traffic.

### Interaction with existing surfaces

- **Seed:** seeded objects do **not** fire notifications (startup fixtures,
  written before bind). Seed also does **not** declare notification config —
  `seed.yaml` stays "buckets + finished objects" only, per its spec; config is
  created through the UI/seam into SQLite.
- **UI-originated uploads/deletes DO fire notifications** (resolved). A developer
  who adds or deletes a file in the bucket browser expects it to behave like a
  real mutation — including triggering their webhook — so UI writes/deletes fire,
  the same as an S3-client mutation. This is an **intentional divergence from the
  live log**, which excludes UI mutations: the log answers "what did my *app*
  do?", while a notification models "an object landed/left" regardless of who
  caused it. (Firing at the store layer makes this automatic — see *Event set &
  resolution*.)
- **CORS / `--cors`:** unrelated — CORS governs a browser calling cubby's S3 API
  cross-origin; webhooks are cubby → the app's server. They ship in the same
  v0.2 theme but are independent.

## Integration recipes (informative)

Not acceptance criteria — guidance for the README/docs so the feature lands for
real setups. cubby always does the same thing: POST the S3 event (body shaped by
`format`) to a `url`. The recipe is just *what runs at that url* to stand in for
the prod routing target (the spec's "delivery edge" the developer owns).

- **A plain HTTP endpoint in your app (the common case).** Your service exposes a
  route (`POST /s3-hook`) that deserializes the body and runs your logic. Point
  the destination `url` at it. Works for any language/framework; nothing
  AWS-specific.

- **A Lambda function, via an in-app bridge (recommended for a mono-repo).** When
  the function shares a repo with a web app, add a **dev-only** endpoint to the
  web app that deserializes cubby's `s3-notification` body into the SDK's S3-event
  type and calls the *same handler method* the Lambda uses — e.g. in .NET,
  `JsonSerializer.Deserialize<Amazon.Lambda.S3Events.S3Event>(body)` →
  `new Function().FunctionHandler(evt, new TestLambdaContext())`. Point the `url`
  at that route (`http://localhost:<webapp-port>/dev/s3-event`). This runs the
  real handler **in-process and debuggable**, with **no Lambda tooling** — the
  right answer when an interactive-only tool (see below) can't be driven as an
  HTTP target. Fast, so the default `timeout_ms` is fine.

- **A Lambda function, via AWS SAM Local.** `sam local start-lambda` exposes the
  Lambda Invoke API (default `http://127.0.0.1:3001`); set the destination `url`
  to the invoke path
  `http://127.0.0.1:3001/2015-03-31/functions/<FunctionName>/invocations`. cubby's
  POST body *is* the invoke payload, so SAM runs your handler in its container
  with real S3-event data. Higher runtime fidelity, heavier than the bridge.
  **Raise `timeout_ms`** (e.g. `20000`) — a container cold start can exceed the
  5000 default and otherwise logs a spurious timeout even though the invoke ran.

- **Interactive Lambda testers (e.g. the .NET Mock Lambda Test Tool).** These are
  built for manual, IDE-attached invocation (paste a payload and run), **not** as
  an unattended HTTP push target, so cubby cannot deliver to them automatically.
  Use the in-app bridge or SAM Local above instead; keep the interactive tool for
  the manual **replay** loop — cubby fires and shows the event JSON in its live
  log/UI, which you copy into the tool to invoke by hand under the debugger.

## Acceptance criteria

Named observers: a **local HTTP receiver** — a tiny test endpoint (e.g. a
`python -m http.server`-style handler, or a few-line recorder in
`tests/acceptance/`) that captures the method, path, headers, and body of what
cubby POSTs; **real S3 clients** (AWS CLI / boto3) to generate the mutations;
**`curl`** against the `/_/api/…/notifications` seam and a **human in the browser**
for the config-management boxes; and the existing **live-log/ndjson** stream
(`/_/api/events`) and **filesystem/SQLite** for cross-checks. Each box becomes a
plan checkbox.

### Firing & payload
- [ ] **PutObject fires an `ObjectCreated:Put` webhook.** With a `notify`
      destination on bucket `uploads` for `s3:ObjectCreated:*`, an
      `aws s3 cp file.txt s3://uploads/photos/cat.jpg` causes the receiver to get
      exactly one `POST` whose body is the `s3-notification` shape with
      `eventName:"ObjectCreated:Put"`, `s3.bucket.name:"uploads"`,
      `s3.object.key:"photos/cat.jpg"`, and an `s3.object.size`/`eTag` equal to
      what `aws s3api head-object` reports.
- [ ] **DeleteObject fires an `ObjectRemoved:Delete` webhook.** Deleting that
      key (`aws s3 rm s3://uploads/photos/cat.jpg`) delivers one POST with
      `eventName:"ObjectRemoved:Delete"` and no `size`/`eTag`.
- [ ] **Multipart completion fires `CompleteMultipartUpload`.** A boto3
      `upload_file` of a >8MB body (forces multipart) delivers **one**
      `ObjectCreated:CompleteMultipartUpload` event (not one per part) whose
      `eTag` is the `-N` composite ETag `head-object` returns.
- [ ] **CopyObject fires `ObjectCreated:Copy`.** `aws s3 cp s3://uploads/a
      s3://uploads/b` delivers an `ObjectCreated:Copy` for key `b`.
- [ ] **DeleteObjects fires one event per key.** A batch delete of three keys
      delivers three `ObjectRemoved:Delete` POSTs (or three Records), one per
      removed key.
- [ ] **UI mutations fire, too (diverges from the live log).** With a matching
      destination on `uploads`, uploading a file into it via the bucket
      browser (`PUT /_/api/buckets/uploads/objects/…`) delivers an
      `ObjectCreated:Put` webhook, and deleting it via the UI delivers an
      `ObjectRemoved:Delete` — even though neither appears in the live-log
      stream. Proves notifications fire at the store layer regardless of origin.

### Format selector
- [ ] **`format: eventbridge` yields the EventBridge shape.** A destination with
      `format: eventbridge` receives a body with top-level `source:"aws.s3"`,
      `detail-type:"Object Created"`, and `detail.object.key` (lowercase `etag`),
      for the *same* PutObject that a `s3-notification` destination renders as
      `{"Records":[…]}`. Both shapes for one event, side by side, prove the
      selector.
- [ ] **Default format is `s3-notification`.** A destination omitting `format`
      receives the `{"Records":[…]}` shape.

### Filtering
- [ ] **Prefix filter gates delivery.** With `prefix: photos/`, a PUT to
      `photos/cat.jpg` fires but a PUT to `docs/readme.md` does **not** (receiver
      records exactly one POST across both).
- [ ] **Suffix filter gates delivery.** With `suffix: .jpg`, `a.jpg` fires and
      `a.png` does not.
- [ ] **Event filter gates delivery.** A destination subscribed only to
      `s3:ObjectCreated:*` receives the PUT but **not** the subsequent DELETE of
      the same key.
- [ ] **Per-path routing to different destinations.** With two destinations on
      bucket `uploads` — `prefix: photos/` → receiver A, `prefix: invoices/` →
      receiver B — a PUT to `photos/cat.jpg` reaches **only** A and a PUT to
      `invoices/2026.pdf` reaches **only** B. This is the per-path story: one
      bucket, path-scoped routing to distinct URLs.
- [ ] **Overlapping filters fan out (cubby diverges from AWS).** Two destinations
      whose filters both match `photos/cat.jpg` (e.g. `prefix: photos/` and
      `suffix: .jpg`) each receive the PUT — one object event, two POSTs — rather
      than being rejected as an ambiguous config the way AWS would.

### Delivery semantics (the dev-tool guarantees)
- [ ] **A dead receiver never blocks the client.** With a destination `url`
      pointing at a closed port (or a receiver that sleeps 30s), an
      `aws s3 cp … s3://uploads/k` still returns success promptly (well under the
      hang), and the object is on disk (`cat s3data/buckets/uploads/k`) — the
      write path is unaffected. The failed delivery is logged.
- [ ] **`timeout_ms` is honored per destination.** A destination with
      `timeout_ms: 500` pointed at a receiver that sleeps ~2s logs a timeout (no
      2xx), while an otherwise-identical destination with `timeout_ms: 5000`
      against the same receiver delivers successfully — proving the bound is
      per-destination, and that omitting it applies the 5000 default.
- [ ] **Delivery is visible in the live log.** After a firing PUT,
      `curl -N '/_/api/events?format=ndjson'` (or stdout) shows a synthetic
      webhook-delivery line naming the destination and its response status,
      adjacent to the `PutObject` event.
- [ ] **No config, no behavior change.** `cubby serve <dir>` on a bucket with no
      destinations never POSTs anywhere and behaves exactly as today (baseline
      that the feature is opt-in; startup is unchanged).

### Config management (SQLite + UI + seam)
- [ ] **Add a destination via the seam, it persists.**
      `POST /_/api/buckets/uploads/notifications` with a valid destination body
      returns `201` with an `id`; a subsequent `GET …/notifications` lists it;
      and a row exists in `bucket_notifications` (verifiable via the DB or a
      restart — see persistence box).
- [ ] **Add a destination in the UI, then it fires — no restart.** In the bucket
      browser's Notifications panel, a human adds a destination, then (same
      running server) an `aws s3 cp … s3://uploads/photos/x.jpg` delivers a POST
      to it. Config is live immediately; nothing was restarted.
- [ ] **Delete a destination, it stops firing.**
      `DELETE /_/api/buckets/uploads/notifications/{id}` (or the UI row delete) →
      `GET …/notifications` no longer lists it, and a subsequent matching PUT
      delivers **no** POST.
- [ ] **Invalid destination is rejected at write time.** `POST …/notifications`
      with an unknown event name, an invalid `format`, or a non-`http://` `url`
      (including an `https://` one — out of scope for v0.2) returns `400` with a
      naming error and persists nothing (`GET` still empty). (Replaces any
      startup "fail-fast" — startup is unchanged.)
- [ ] **Config survives restart and travels with the data dir.** After adding a
      destination, stopping cubby, and re-serving the **same** data dir, the
      destination is still listed and still fires — proving it's durable SQLite
      state, not in-memory.
- [ ] **Deleting the bucket removes its config.** After `aws s3 rb s3://uploads`
      (bucket delete), no `bucket_notifications` rows for `uploads` remain
      (cascade), so a re-created same-named bucket starts with no destinations.

### Fidelity (handler-runs-unchanged)
- [ ] **The captured payload parses as an AWS event.** The `s3-notification`
      body deserializes with a real AWS SDK's S3-event type (e.g. Go
      `events.S3Event` from `aws-lambda-go`, or boto3-style dict access
      `body["Records"][0]["s3"]["object"]["key"]`) with the expected key/size —
      proving the shape is real, not merely "JSON with our fields".

## Resolved decisions

Confirmed by the user — none of these block planning.

1. **Config location → per-bucket state in SQLite.** ✅ Created/viewed/deleted
   through the Web UI and a thin `/_/api/…/notifications` seam, live without
   restart. No `--notify` file, no `seed.yaml` extension.
2. **UI-originated uploads/deletes DO fire notifications.** ✅ Developers expect
   adding/deleting a file in the bucket browser to trigger their webhook, so
   notifications fire at the **store layer** regardless of origin. Intentional
   divergence from the live log (which excludes UI mutations). See *Interaction
   with existing surfaces*.
3. **Delivery: exactly one attempt, no retry.** ✅ A single POST bounded by the
   destination's `timeout_ms` (**default 5000**, per-destination overridable);
   failures are logged and dropped, never retried, never surfaced to the S3
   client. The per-destination timeout lets a slow, cold-starting target (e.g.
   SAM Local) get headroom without changing the fast-receiver default.
5. **Shared-secret / auth header → deferred.** ✅ v0.2 is a plain POST to a local
   dev endpoint. An optional per-destination auth header is a noted fast-follow,
   not in this spec.
6. **`sequencer` → a local monotonic counter.** ✅ Emit a monotonic hex counter
   (its own space, or the event-bus id space) so the field is present and
   correctly typed; documented as a local monotonic, not AWS's value.
7. **Repo/CI portability → local-only for v0.2 (option a).** ✅ Config lives in
   SQLite and the data dir is `.gitignore`'d, so UI-set config does **not** travel
   with the repo; a clean checkout / CI dir starts with no destinations. Accepted
   for v0.2. A declarative bootstrap (a seed-style file that pre-populates the
   table) is a fast-follow if the "reproducible fixture" case demands it.
8. **UI placement → per-bucket Notifications panel in the bucket browser.** ✅
   Config is bucket-scoped, so it belongs with the bucket view; keeps the
   two-item nav from the Phase 5 mockups.
4. **Outbound delivery → `http://` only for v0.2.** ✅ Webhook `url`s must be
   `http://` (an `https://` url is rejected at write time). Rationale: the tool's
   primary user only needs plain HTTP to local receivers. This keeps cubby's
   first outbound-HTTP capability to a **thin client on the `hyper` already in the
   tree** (plus `tokio::time::timeout`) with **no TLS stack and no heavy new
   dependency** — no rustls/OpenSSL, so the static-musl / distroless build is
   untouched. `https://` delivery is a later fast-follow if ever needed. (Final
   crate wiring settles in `/plan`; the http-only scope is fixed here.)

## Open questions

None — all resolved above. Ready for `/plan event-notifications`.
