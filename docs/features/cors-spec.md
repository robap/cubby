# CORS — spec

**Status:** done · **Roadmap:** v0.2 ("Browser-facing & workflow"; CONCEPT lists
CORS as a v0.2 fast-follow and a known sharp edge the presign button provokes.
**This spec supersedes the "`--cors` flag" framing in CONCEPT/ROADMAP** — both docs
have been updated to describe the per-bucket API) · **Slug:** `cors`

## Why

The real browser→S3 flow is not "click cubby's presign button, then hand-call
`fetch`." It is: a developer's **backend** generates a presigned URL
programmatically (SDK — `generate_presigned_url` / `getSignedUrl`), hands it to
the **frontend**, and the frontend `fetch()`es it cross-origin — a React app on
`http://localhost:3000` uploading directly to, or downloading from, cubby's S3 API
on `http://localhost:9000`. cubby's own presign button is a *debugging aid*, not
the flow.

For that cross-origin `fetch()` to survive the browser's same-origin policy, the
**bucket** it targets must have CORS configured — and in AWS, CORS is **per-bucket
configuration set through the S3 API**: `PutBucketCors` / `GetBucketCors` /
`DeleteBucketCors`, an XML `CORSConfiguration` of `CORSRule`s (`AllowedOrigins`,
`AllowedMethods`, `AllowedHeaders`, `ExposeHeaders`, `MaxAgeSeconds`). A fresh
bucket has no CORS; the developer adds it in their terraform/CDK or a bootstrap
`aws s3api put-bucket-cors`. It is mutable bucket state, not a server setting.

cubby should honor that **same** API. Doing so beats the originally-sketched
process-level `--cors` flag on every axis that matters here:

- **It's what developers already run.** Their existing `put-bucket-cors` bootstrap
  or IaC works unchanged against cubby — no cubby-specific setup, no divergence
  between local and prod. That is *compatibility proven, not claimed*.
- **It travels and it's runtime-mutable.** The config lives in `meta.sqlite` inside
  the data dir, so it moves when the dir is copied and changes with no restart —
  which a **container** deployment needs (a startup flag would mean editing the
  container's command/env and restarting just to change an allowed origin).
- **It doesn't lie.** A global "allow everything" flag would make a browser upload
  pass against cubby while silently hiding that the developer never configured the
  bucket's CORS — so it breaks in prod. For a tool whose whole point is catching
  exactly these S3 mistakes locally, faithfully reproducing "no bucket CORS → the
  browser blocks it, and you can see *why* in the live log" is the feature.

And it's cheap: **`s3s` already implements the CORS wire format** — the
`put_bucket_cors`/`get_bucket_cors`/`delete_bucket_cors` trait methods and the
`CORSRule` XML DTO exist and are merely stubbed `NotImplemented`. cubby overrides
three trait methods against SQLite, the same "s3s owns the protocol, we own the
filesystem+SQLite backend" pattern as every other operation.

North stars served:
- **Compatibility is proven, not claimed.** A real SDK/CLI sets the bucket's CORS
  and a **real browser** then completes a cross-origin presigned upload — the
  acceptance test drives both, because only a browser actually *enforces* CORS.
- **Dev tool first.** The frontend-upload flow works locally exactly as in prod,
  and a *rejected* preflight is a visible live-log line ("why did my browser upload
  fail") instead of an opaque browser console error — the S3-debugger promise.
- **The filesystem is the API too / starts in ms, zero config.** CORS config is
  bucket state in the data dir; a bucket with none behaves exactly as a fresh S3
  bucket (and exactly as cubby does today). The feature is dormant until a client
  sets a rule.

> **Doc note:** CONCEPT.md and ROADMAP.md previously described CORS as a `--cors`
> flag. Adopting the real per-bucket API supersedes that; both docs have been
> updated to match.

## What CORS is (and where cubby sits)

A browser making a cross-origin request to cubby does up to two things:

1. **Preflight** (for anything beyond a "simple" request — custom headers like
   `Authorization`/`x-amz-*`, or methods like `PUT`/`DELETE`): the browser first
   sends an **unauthenticated `OPTIONS`** to the target URL carrying `Origin`,
   `Access-Control-Request-Method`, and `Access-Control-Request-Headers`. It expects
   a response whose `Access-Control-Allow-*` headers *cover* the intended request.
   If they don't, the browser never sends the real request.
2. **Actual request** (the presigned `PUT`/`GET`): the browser sends it, and
   requires the *response* to carry `Access-Control-Allow-Origin` matching the page
   origin before it lets JS read the result. To read response headers like `ETag`
   (needed to confirm an upload / drive browser-side multipart), the response must
   also list them in `Access-Control-Expose-Headers`.

AWS answers both by evaluating the target bucket's stored `CORSRule`s (first
matching rule wins). cubby reproduces that: the rules are per-bucket SQLite state,
set via the S3 API; the routing layer answers preflights and stamps actual
responses from those rules.

## In scope

- **The three S3 CORS operations, backed by SQLite bucket state:**
  - **`PutBucketCors`** — accept the `CORSConfiguration` XML (parsed by `s3s`),
    validate it, and store the bucket's rule list as bucket state. Replaces any
    existing config for that bucket (AWS semantics: put is whole-config replace).
  - **`GetBucketCors`** — return the stored `CORSConfiguration` XML; return the S3
    error **`NoSuchCORSConfiguration`** (`404`) when the bucket has none.
  - **`DeleteBucketCors`** — remove the bucket's config (idempotent).
  - Config is **mutable at runtime, no restart**, and **travels with the data dir**
    (it's rows in `meta.sqlite`). Deleting a bucket **cascades** its CORS config
    away, like its other bucket state.
- **Preflight (`OPTIONS`) enforced at the routing layer, before auth.** A
  cross-origin `OPTIONS` carrying `Origin` + `Access-Control-Request-Method` is
  matched against the target bucket's rules and answered **before** the `s3s` SigV4
  layer — a preflight carries no signature, so it must never `403` for *lack of
  auth*. On a matching rule: respond with `Access-Control-Allow-Origin`,
  `Access-Control-Allow-Methods`, `Access-Control-Allow-Headers`,
  `Access-Control-Max-Age`, and `Access-Control-Expose-Headers` derived from the
  matched rule. On **no** match (or a bucket with no config): respond as AWS does —
  `403` with a CORS error body and **no** `Access-Control-Allow-Origin`, so the
  browser blocks the pending request.
- **Allow-origin headers added to actual S3 responses.** When a request bears an
  `Origin` that a bucket rule matches, cubby adds `Access-Control-Allow-Origin`
  (echoed origin + `Vary: Origin`, or `*` when the matched rule's origin is exactly
  `*`) and `Access-Control-Expose-Headers` (from the rule) to the S3 response —
  **success and error alike**, so a browser can read both. CORS headers are purely
  additive; cubby does not change the S3 status, body, or signature verification.
- **AWS rule-matching semantics** (the parts SDK-generated configs rely on;
  exact edge behavior settled in `/plan`): rules evaluated in order, **first match
  wins**; `AllowedOrigins` supports a single `*` wildcard segment
  (`https://*.example.com`) and the bare `*`; `AllowedMethods` ∈
  {`GET`,`PUT`,`POST`,`DELETE`,`HEAD`}; `AllowedHeaders` supports `*` and is matched
  (case-insensitively) against the preflight's `Access-Control-Request-Headers`;
  `MaxAgeSeconds` and `ExposeHeaders` are echoed into the response.
- **Live-log visibility.** A CORS preflight is recorded as a live-log/stdout event
  (method `OPTIONS`, a resolved op such as `Preflight`, the requesting origin, and
  whether a rule allowed it) so "my browser upload is failing preflight" is answered
  in the same stream as S3 traffic.
- **Read-only CORS display in the web UI.** When a bucket is selected in the bucket
  browser, the UI shows that bucket's currently-configured CORS rules (origins,
  methods, allowed/expose headers, max-age) — or "no CORS configured" — read from a
  thin `GET /_/api/buckets/{bucket}/cors` seam (the same read-only JSON pattern the
  rest of the UI uses; no SigV4 in the UI). It reflects the live config with **no
  restart**: after an `aws s3api put-bucket-cors`, reloading the bucket shows the new
  rules. **Display only** — the UI does not create/edit/delete CORS (management stays
  the real S3 API, the fidelity point); it makes the config *visible* so a developer
  can confirm at a glance which origins a bucket allows.
- **A short docs section** (README / known-sharp-edges): configure CORS with
  `aws s3api put-bucket-cors` (with an example rule set); the presigned-URL
  **host-in-signature** gotcha (a URL signed for `localhost:9000` must be fetched at
  `localhost:9000`, not `127.0.0.1:9000` — same origin-mismatch class as the Docker
  note); and that `localhost:3000` vs `localhost:9000` are different origins on
  purpose.

## Out of scope

- **A process-level `--cors` flag / any global allow-all switch.** Superseded by
  the per-bucket API (see *Why*). No `--cors`, no `CUBBY_CORS`. A fresh bucket has
  no CORS, exactly like S3.
- **`Access-Control-Allow-Credentials` / cookie-authenticated CORS.** Presigned URLs
  authenticate via the query signature, not cookies, so credentialed CORS isn't part
  of this flow. cubby does not negotiate credentials mode in v0.2. (A later
  fast-follow if some flow ever needs it; noted, not built.)
- **CORS for the `/_/` web-UI JSON/SSE seam.** The UI is served by cubby itself, so
  its `/_/api/*` calls are same-origin. This spec governs the **S3 wire surface**
  only.
- **Creating/editing/deleting CORS *through the UI*.** The bucket browser **displays**
  a bucket's CORS config (read-only — in scope above) but does **not** mutate it;
  management is the real S3 API (`PutBucketCors`/`DeleteBucketCors`), which is the
  fidelity point. No write endpoint under `/_/api/…/cors`.
- **Declaring CORS in `seed.yaml`.** Seed stays "buckets + finished objects" per its
  spec; CORS is set through the API at runtime. (A declarative seed of CORS config
  is a fast-follow only if the reproducible-fixture case demands it.)
- **Per-object CORS** (S3 has no such thing) and **CORS on service-level requests**
  (`ListBuckets` at `/`) — CORS is bucket-scoped.

## Behavior

### Configuring a bucket's CORS (the S3 API)

```
aws s3api put-bucket-cors --bucket uploads --cors-configuration '{
  "CORSRules": [{
    "AllowedOrigins": ["http://localhost:3000"],
    "AllowedMethods": ["GET","PUT","POST","HEAD"],
    "AllowedHeaders": ["*"],
    "ExposeHeaders": ["ETag"],
    "MaxAgeSeconds": 600
  }]
}'
```

- `s3s` parses/serializes the XML; cubby validates (at least one rule; each rule has
  ≥1 origin and ≥1 method; methods ∈ the S3 set) and **stores the rule list as
  bucket state**. `PutBucketCors` **replaces** the whole config.
- `GetBucketCors` returns it; on a bucket with none, `NoSuchCORSConfiguration`
  (`404`) — the exact error AWS returns, so an SDK's "does this bucket have CORS?"
  probe behaves normally.
- `DeleteBucketCors` removes it, and is **idempotent** — deleting when none
  exists is **not** an error; cubby returns AWS's tolerant `204` (resolved).

**Storage** — a new `meta.sqlite` table (sketch; exact columns settled in `/plan`),
CORS config as first-class bucket state that cascades on bucket delete:

```
bucket_cors(
  bucket   TEXT NOT NULL,   -- FK → buckets(name), ON DELETE CASCADE
  rules    TEXT NOT NULL,   -- the CORSRule list, serialized (JSON)
  ...                       -- (single-row-per-bucket, or one row per rule; /plan decides)
)
```

### Preflight — the unsigned `OPTIONS`, answered before auth

An `OPTIONS` carrying `Origin` **and** `Access-Control-Request-Method` is a CORS
preflight. cubby resolves the target bucket from the path, loads its rules, and
finds the **first rule** matching the origin + requested method (+ requested
headers). On a match: `204` with `Access-Control-Allow-Origin` (echoed origin, or
`*` for a `*` rule), `Access-Control-Allow-Methods`, `Access-Control-Allow-Headers`,
`Access-Control-Max-Age`, `Access-Control-Expose-Headers`, and `Vary: Origin`. On no
match / no config: `403` with a CORS error body and no allow-origin header (browser
blocks). This is decided at the routing layer, **before** `s3s`, so it never
depends on a signature. An `OPTIONS` **without** the preflight headers is not a
preflight and falls through unchanged.

### Actual cross-origin request

A real request (e.g. a presigned `GET`/`PUT`) flows through `s3s` as normal
(signature verified there); afterward, if it bears an `Origin` matched by a bucket
rule, cubby adds `Access-Control-Allow-Origin` (+ `Vary: Origin`) and
`Access-Control-Expose-Headers` to the response — success and error. No `Origin`,
or no matching rule → no CORS headers (cubby still serves the request server-side;
the browser is what blocks JS from reading it — correct CORS semantics).

### Interaction with existing surfaces

- **Presign button (Phase 5).** Unchanged. The URL it mints is signed for cubby's
  own host:port; a browser page on another origin can now fetch it *iff* the
  bucket's CORS allows that page origin. The host-in-signature gotcha applies
  (documented).
- **Live log.** Preflights become visible events; actual cross-origin requests log
  as their normal op (the added CORS headers don't change `op`/status).
- **Event notifications (v0.2 sibling).** Independent. Webhooks are cubby→app
  server-to-server (no browser, no CORS); CORS is browser→cubby. Shared theme, no
  shared surface.
- **Seed / accept-any-credentials / `--bind`.** Orthogonal. CORS is not
  authentication; it governs which *page origins* a browser will let read cubby's
  responses.

## Acceptance criteria

Named observers: **AWS CLI / boto3** driving `put-bucket-cors` / `get-bucket-cors` /
`delete-bucket-cors` (proves the config API is real and round-trips — compatibility
proven); **`curl`** sending `Origin`/preflight headers to assert the exact response
contract deterministically; a **real browser via Playwright MCP** (the only
observer that actually *enforces* CORS) loading a page from a **second origin**
(e.g. `python -m http.server 3000`, cross-origin to cubby on `:9000`) and doing a
real cross-origin `fetch()`; **SQLite + a restart** for persistence; **the presign
seam/button** to mint a real presigned URL; the **live-log ndjson** stream
(`/_/api/events?format=ndjson`); and the **filesystem** (`cat s3data/buckets/…`) to
confirm bytes on an allowed upload. Each becomes a plan checkbox.

### Config API (compatibility — a real client sets/reads CORS)
- [ ] **Put then Get round-trips the rules.** `aws s3api put-bucket-cors --bucket
      uploads --cors-configuration '{…}'` succeeds, and `aws s3api get-bucket-cors
      --bucket uploads` returns the same rules (origins, methods, allowed/expose
      headers, max-age) — proving the XML shape is real, not merely "we stored
      something."
- [ ] **Get on a bucket with no CORS returns `NoSuchCORSConfiguration`.**
      `get-bucket-cors` on a fresh bucket fails with that S3 error code (`404`), the
      exact behavior an SDK expects.
- [ ] **Delete removes it.** `aws s3api delete-bucket-cors --bucket uploads` →
      a subsequent `get-bucket-cors` again returns `NoSuchCORSConfiguration`, and a
      previously-working cross-origin fetch (below) is now blocked.
- [ ] **Config persists across restart and travels with the data dir.** After
      `put-bucket-cors`, stopping cubby and re-serving the **same** data dir,
      `get-bucket-cors` still returns the rules and preflight still passes — proving
      it's durable SQLite state, not in-memory.
- [ ] **Deleting the bucket removes its CORS.** After `aws s3 rb s3://uploads`, no
      `bucket_cors` rows for `uploads` remain (cascade); a re-created same-named
      bucket starts with no CORS.

### Preflight (server-side contract, via `curl`)
- [ ] **Preflight is answered without auth when a rule matches.** With a rule
      allowing `http://localhost:3000` + `PUT`, `curl -i -X OPTIONS
      'http://localhost:9000/uploads/photos/cat.jpg' -H 'Origin: http://localhost:3000'
      -H 'Access-Control-Request-Method: PUT' -H 'Access-Control-Request-Headers:
      authorization,content-type'` returns `204` (not `403`) with
      `Access-Control-Allow-Origin: http://localhost:3000`, `Access-Control-Allow-Methods`
      including `PUT`, an `Access-Control-Allow-Headers` covering the requested
      headers, and a non-zero `Access-Control-Max-Age` — despite carrying no
      signature.
- [ ] **A non-matching origin is refused.** The same preflight with `Origin:
      http://evil.test` returns `403` with **no** `Access-Control-Allow-Origin`
      header (browser would block).
- [ ] **A non-allowed method is refused.** With a rule allowing only `GET`, a
      preflight requesting `DELETE` returns no `Access-Control-Allow-Origin`.
- [ ] **A `*` origin rule allows any origin.** With `AllowedOrigins:["*"]`, a
      preflight from any `Origin` yields `Access-Control-Allow-Origin: *`.

### Actual-request headers (server-side contract, via `curl`)
- [ ] **Allow-origin + expose-headers on a real response.** A presigned `GET` (URL
      from the presign seam) fetched with `curl -i -H 'Origin: http://localhost:3000'`
      against a bucket whose rule allows that origin returns `200` **and**
      `Access-Control-Allow-Origin: http://localhost:3000` with
      `Access-Control-Expose-Headers` listing `ETag`.
- [ ] **Allow-origin present on errors too.** A cross-origin request S3 answers
      `403`/`404` still carries `Access-Control-Allow-Origin`, so a browser can read
      the failure.
- [ ] **No config, no CORS headers (baseline).** On a bucket with no CORS, a normal
      signed `GET` response carries no `Access-Control-*` headers and an `OPTIONS`
      preflight is refused — identical to a fresh S3 bucket and to cubby today.

### End-to-end in a real browser (the CORS-enforcing observer, via Playwright)
- [ ] **A cross-origin presigned upload succeeds when the bucket allows the origin.**
      With `uploads` CORS allowing `http://localhost:3000` (`PUT`) and a static page
      served on `:3000`, Playwright loads the page and runs
      `fetch(presignedPutUrl,{method:'PUT',body})`; the promise **resolves** with
      `res.ok`, and `cat s3data/buckets/uploads/<key>` then shows the bytes —
      proving the browser sent the real PUT because the preflight passed.
- [ ] **A cross-origin presigned download is readable.** From the same page,
      `fetch(presignedGetUrl)` **resolves** and JS reads `res.headers.get('ETag')`
      and the body — proving allow-origin + expose-headers let the browser expose the
      response.
- [ ] **Without matching CORS, the same fetch is blocked.** After
      `delete-bucket-cors` on `uploads` (or with a rule allowing only a different
      origin), the identical cross-origin `fetch()` from the `:3000` page **rejects**
      with a CORS/`TypeError`, and the object is **not** written on the blocked PUT —
      demonstrating that the bucket's CORS config is what enables the flow.

### Live-log visibility
- [ ] **A rejected preflight is visible in the live log.** A preflight from a
      disallowed origin emits a line in `curl -N
      'http://localhost:9000/_/api/events?format=ndjson'` naming the
      `OPTIONS`/preflight op and the requesting origin (and that it was rejected), so
      a developer sees *why* their browser request failed in cubby's own stream.

### UI display (read-only, via Playwright / a human in the browser)
- [ ] **The bucket browser shows a bucket's CORS config, live.** After
      `aws s3api put-bucket-cors --bucket uploads …`, selecting `uploads` in the
      bucket browser (same running server, no restart) shows the configured rules
      (the allowed origin, methods, and expose-headers are visible on screen); a
      bucket with none shows a "no CORS configured" state. After
      `delete-bucket-cors`, reloading the bucket reflects the removal. The UI offers
      no add/edit/delete control for CORS (display only).

## Resolved decisions

Confirmed by the user — none block planning.

1. **Mechanism → the real per-bucket S3 CORS API**, not a `--cors` flag.
   `PutBucketCors`/`GetBucketCors`/`DeleteBucketCors`, SQLite-backed bucket state.
   Reason: matches what developers already run (their `put-bucket-cors`/IaC works
   unchanged), travels with the data dir, runtime-mutable (container-friendly), and
   doesn't hide a missing-CORS misconfig behind a local switch.
2. **CONCEPT.md and ROADMAP.md updated** to describe the per-bucket API (done as part
   of this refine; the `--cors` flag language is gone).
3. **UI shows the configured CORS, read-only.** The bucket browser displays a
   bucket's rules (in scope); it does not edit them — management stays the S3 API.
4. **`DeleteBucketCors` when none exists → tolerant `204`**, mirroring AWS (not
   `NoSuchCORSConfiguration`).

## Open questions

None — all resolved above. Ready for `/plan cors`.
