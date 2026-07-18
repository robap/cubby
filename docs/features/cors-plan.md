# CORS — plan

**Spec:** [cors-spec.md](cors-spec.md) · **Roadmap:** v0.2 ("Browser-facing & workflow")

## Approach

cubby implements the **real per-bucket S3 CORS API** by overriding three `s3s`
trait methods that today return `NotImplemented` — `put_bucket_cors`,
`get_bucket_cors`, `delete_bucket_cors` — against a new `bucket_cors` SQLite
table, exactly the "s3s owns the wire protocol/XML, we own filesystem+SQLite"
pattern every other op follows (*compatibility proven, not claimed*). Config is
one row per bucket (`PutBucketCors` is a whole-config replace), stored as JSON,
cascading on bucket delete like `bucket_notifications`.

Enforcement is two touch-points in `http.rs`, the layer that already sits *above*
`s3s`: (1) a **preflight** short-circuit in `Router::call` that answers an unsigned
`OPTIONS` **before** the SigV4 layer (a preflight has no signature, so it must
never 403 for lack of auth), and (2) **response-header injection** in
`log_and_serve_s3` that stamps `Access-Control-Allow-Origin`/`-Expose-Headers`
onto an actual cross-origin response. Both consult the bucket's rules through a
pure, unit-tested matching module `src/cors.rs` (first-match-wins, origin
wildcards, header negotiation) — isolating the quirky part the way `listing.rs`
and `multipart.rs` isolate theirs. Normal SDK traffic carries no `Origin` header,
so both touch-points are **guarded on `Origin` presence** — zero added cost and
zero behavior change when the feature is dormant (*starts in ms, zero config*).

The UI gets a **read-only** display: a thin `GET /_/api/buckets/{bucket}/cors`
seam reading the same table, surfaced in the bucket browser (management stays the
S3 API — the fidelity point). Preflight decisions surface as synthetic live-log
events via the existing `note` mechanism, so "why did my browser upload fail" is
answered in the same stream as S3 traffic (*dev tool first / S3 debugger*).

## Files

- `src/db.rs` — add the `bucket_cors` table to `SCHEMA_V0` (one row per bucket,
  `bucket` PK, `REFERENCES buckets(name) ON DELETE CASCADE`); add
  `put_bucket_cors` (INSERT OR REPLACE), `get_bucket_cors` (→ `Option<String>`
  JSON), `delete_bucket_cors` (idempotent DELETE) + tests.
- `src/cors.rs` — **new.** Pure domain types (`CorsRule`, serde-(de)serializable)
  and the matching engine: `match_preflight(&rules, origin, method, req_headers)
  -> Option<PreflightGrant>` and `match_actual(&rules, origin, method) ->
  Option<ActualGrant>`; origin wildcard + header/method matching; first-match-wins.
  Unit tests live here.
- `src/lib.rs` — `mod cors;`.
- `src/store.rs` — implement the three `s3s::S3` CORS methods; convert the `s3s`
  `CORSRule` DTO ↔ `cors::CorsRule`; validate on put; `NoSuchCORSConfiguration` on
  get-when-empty; tolerant delete.
- `src/http.rs` — preflight short-circuit in `Router::call`; CORS-header injection
  + preflight live-log emit in `log_and_serve_s3` (capture `Origin`/method before
  the body is consumed).
- `src/api/cors.rs` — **new.** Read-only `GET /_/api/buckets/{bucket}/cors`
  (`{ "cors": [...] | null }`); non-GET → `405`.
- `src/api/mod.rs` — `mod cors;`, a `parse_cors_path`, and a dispatch arm ordered
  before the objects fallback (like `parse_notifications_path`).
- `web/src/lib/api.ts` — `CorsInfo` type + `getCors(bucket)`.
- `web/src/stores/cors.ts` — **new.** Read-only store (load a bucket's CORS on
  select).
- `web/src/components/cors-panel.ts` — **new.** Read-only display ("no CORS
  configured" empty state).
- `web/src/routes/browser.ts` — surface the CORS display in the bucket view.
- `web/styles/app.scss` — panel styling as needed.
- `web/dist/**` — regenerated with `zero build` (committed build artifact; CI has
  no `zero`).
- `README.md` — CORS section (see docs step).

## Risks & unknowns

- **Preflight must precede `s3s` auth.** Placing the check in `Router::call` before
  the S3 handoff is correct, but it must be *strictly* guarded on
  `OPTIONS` + `Origin` + `Access-Control-Request-Method` so a non-CORS `OPTIONS`
  (and every normal request) falls through unchanged. Getting the guard wrong
  would change existing behavior.
- **Hot-path DB reads.** Both enforcement points do a `bucket_cors` lookup; guard
  them on `Origin` presence so normal SDK traffic (no `Origin`) pays nothing. The
  routing-layer read is a direct `state.db` call, consistent with how the `/_/api/`
  seam already calls the DB synchronously.
- **s3s DTO ↔ domain conversion.** Confirmed field names: `PutBucketCorsInput.
  cors_configuration.cors_rules`, `GetBucketCorsOutput.cors_rules: Option<CORSRules>`
  (`CORSRules = Vec<CORSRule>`); `CORSRule { allowed_origins, allowed_methods,
  allowed_headers: Option, expose_headers: Option, max_age_seconds: Option, id }`.
  All aliases over `Vec<String>`/`i32`; low risk.
- **Origin wildcard semantics** (bare `*`, single-segment `https://*.example.com`,
  and `*`-rule → `*` vs specific-origin → echo + `Vary: Origin`) are the fiddly
  bit — pinned down entirely inside `cors.rs` with unit tests, so SDK-generated
  configs behave like AWS.
- **`web/dist` is committed** and CI has no `zero` — the UI step isn't done until
  `zero build` has regenerated `dist/` and it's committed (per CONCEPT open Q).
- **Credentials mode out of scope** — never emit `Access-Control-Allow-Credentials`;
  that keeps a `*`-origin grant valid (spec decision).

## Steps

Each box ≈ one small commit moving an observable behavior. Check only when the
outcome is real.

- [x] **`bucket_cors` table + DB methods** — add the table (one row/bucket, FK
      cascade) and `put`/`get`/`delete_bucket_cors`; db unit tests prove
      round-trip, whole-config replace, tolerant delete, and cascade when the
      parent bucket is deleted.
- [x] **Pure matching module `src/cors.rs`** — `CorsRule` (serde) + `match_preflight`
      / `match_actual`; unit tests cover bare `*`, `https://*.example.com`, an
      exact-origin echo, a disallowed method, `AllowedHeaders:["*"]` vs a named
      list (case-insensitive), and first-match-wins across rules.
- [x] **`PutBucketCors` + `GetBucketCors`** — override both in `store.rs`
      (validate ≥1 rule, each with ≥1 origin and ≥1 method; convert DTO↔domain;
      store JSON). Observable: `aws s3api put-bucket-cors …` then
      `aws s3api get-bucket-cors` returns the same rules; `get-bucket-cors` on a
      bucket with none fails `NoSuchCORSConfiguration`.
- [x] **`DeleteBucketCors`** — override in `store.rs`; idempotent (deleting when
      none exists returns `204`). Observable: `delete-bucket-cors` then
      `get-bucket-cors` → `NoSuchCORSConfiguration`; a second delete still succeeds.
- [x] **Preflight answered before auth, with live-log line** — in `Router::call`,
      detect `OPTIONS`+`Origin`+`Access-Control-Request-Method`, resolve the bucket,
      match its rules; on a grant return `204` with `Access-Control-Allow-Origin`/
      `-Allow-Methods`/`-Allow-Headers`/`-Max-Age`/`-Expose-Headers` (+`Vary: Origin`
      for a specific origin), else `403` with a CORS error body and no allow-origin;
      emit a synthetic `Preflight` event (origin + allowed/rejected). Observable:
      the four preflight `curl` acceptance cases, and the rejected preflight appears
      in `/_/api/events?format=ndjson`.
- [x] **CORS headers on actual responses** — in `log_and_serve_s3`, capture the
      request `Origin`/method, and after `s3s` responds, if `Origin` is present and
      the bucket's rules match, add `Access-Control-Allow-Origin` (+`Vary`) and
      `Access-Control-Expose-Headers` to the response (success **and** error).
      Observable: a presigned `GET` with `-H 'Origin: …'` shows the allow-origin +
      `ETag` in expose-headers; a cross-origin `404`/`403` still carries allow-origin;
      no `Origin`/no config → no CORS headers.
- [x] **Read-only CORS seam + UI display** — add `GET /_/api/buckets/{bucket}/cors`
      (non-GET → `405`), wire it in `api/mod.rs`; add `getCors` to `api.ts`, a
      read-only `stores/cors.ts`, a `cors-panel.ts` display, surface it in
      `browser.ts`, and `zero build` → commit `web/dist`. Observable: after
      `put-bucket-cors`, selecting the bucket shows its rules (no restart); none →
      "no CORS configured"; after `delete-bucket-cors` the display reflects removal;
      no add/edit/delete control exists.
- [x] **Docs** — add a README CORS section: configure with an
      `aws s3api put-bucket-cors` example; the presigned-URL **host-in-signature**
      gotcha (sign for and fetch the same `host:port`); and that `localhost:3000`
      vs `localhost:9000` are different origins by design.

## Acceptance

Mirrors the spec. `/implement` isn't done until every box passes by driving the
named client.

### Config API (AWS CLI / boto3)
- [x] `aws s3api put-bucket-cors` then `get-bucket-cors` round-trips the rules
      (origins, methods, allowed/expose headers, max-age).
- [x] `get-bucket-cors` on a bucket with no CORS → `NoSuchCORSConfiguration` (404).
- [x] `delete-bucket-cors` → subsequent `get-bucket-cors` → `NoSuchCORSConfiguration`,
      and a previously-working cross-origin fetch is now blocked.
- [x] Config persists across restart / travels with the data dir: after
      `put-bucket-cors`, stop cubby, re-serve the same dir → `get-bucket-cors` still
      returns the rules and preflight still passes.
- [x] `aws s3 rb s3://uploads` removes the bucket's CORS (cascade): a re-created
      same-named bucket starts with no CORS.

### Preflight (`curl`)
- [x] Preflight with a matching rule returns `204` (not `403`) without auth, with
      `Access-Control-Allow-Origin`, `-Allow-Methods` incl. `PUT`, an `-Allow-Headers`
      covering the requested headers, and a non-zero `-Max-Age`.
- [x] A non-matching `Origin` → `403` with no `Access-Control-Allow-Origin`.
- [x] A non-allowed method (rule allows only `GET`, preflight requests `DELETE`) →
      no `Access-Control-Allow-Origin`.
- [x] An `AllowedOrigins:["*"]` rule → `Access-Control-Allow-Origin: *` for any origin.

### Actual-request headers (`curl`)
- [x] Presigned `GET` with `-H 'Origin: http://localhost:3000'` against an allowing
      bucket → `200` with `Access-Control-Allow-Origin: http://localhost:3000` and
      `Access-Control-Expose-Headers` listing `ETag`.
- [x] A cross-origin `403`/`404` still carries `Access-Control-Allow-Origin`.
- [x] A bucket with no CORS → a normal signed `GET` carries no `Access-Control-*`
      headers and an `OPTIONS` preflight is refused (baseline == today).

### End-to-end in a real browser (Playwright MCP)
- [x] With `uploads` CORS allowing `http://localhost:3000` and a page served on
      `:3000`, `fetch(presignedPutUrl,{method:'PUT',body})` **resolves** with
      `res.ok`, and `cat s3data/buckets/uploads/<key>` shows the bytes.
- [x] From the same page, `fetch(presignedGetUrl)` **resolves** and JS reads
      `res.headers.get('ETag')` and the body.
- [x] After `delete-bucket-cors` (or a rule allowing only a different origin), the
      identical cross-origin `fetch()` **rejects** with a CORS/`TypeError`, and the
      object is **not** written on the blocked PUT.

### Live-log visibility
- [x] A rejected preflight emits a line in `curl -N '/_/api/events?format=ndjson'`
      naming the `OPTIONS`/preflight op and the requesting origin (rejected).

### UI display (Playwright / human)
- [x] After `put-bucket-cors`, selecting `uploads` in the bucket browser shows the
      configured rules live (no restart); a bucket with none shows "no CORS
      configured"; after `delete-bucket-cors` the display reflects removal; no
      add/edit/delete control is present.

## Progress notes

- **Status: done.** All Steps + Acceptance boxes verified.
- **`CorsRule` JSON uses S3 field names.** The domain type serializes with the S3
  `CORSRule` names (`AllowedOrigins`, `AllowedMethods`, `ExposeHeaders`, …) so the
  stored JSON mirrors a `put-bucket-cors` body and the read-only seam/UI speak the
  same vocabulary as `aws s3api get-bucket-cors`. The UI `CorsInfo` type matches.
- **Put-validation acceptance driven via an *unsupported method*, not an
  empty-method rule.** The AWS SDK/CLI enforces "≥1 method" client-side, so the
  server's validation is exercised with an out-of-set method (`PATCH` → `400
  InvalidRequest`) — the invalid-config case a real client can actually send. The
  ≥1-origin / ≥1-method / empty-config paths are still covered by `cors::validate`
  unit tests.
- **Browser acceptance presigned URLs minted via the `/_/api/presign` seam**
  (boto3 wasn't installed); the seam signs a PUT/GET for `localhost:9000`, exactly
  the backend-mints-URL flow the spec describes. Playwright confirmed the
  cross-origin PUT+GET resolve (ETag readable), the bytes landed on disk, and after
  `delete-bucket-cors` the identical PUT rejects with a `TypeError` and no object
  is written.
- **A `CORS` toggle button** sits beside `Notifications` in the bucket-browser
  toolbar; the two panels share the listing pane and are mutually exclusive
  (opening one closes the other; a bucket change closes both).
