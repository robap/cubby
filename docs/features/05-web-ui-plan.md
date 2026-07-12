# Web UI — the S3 debugger — plan

**Spec:** [05-web-ui-spec.md](05-web-ui-spec.md) · **Roadmap:** Phase 5 · **Status:** done

## Approach

Three commit groups (**A** live log → **B** bucket browser → **C** object
detail) behind shared plumbing (**0**), exactly as the spec sequences them. The
UI is a `zero` project in `web/` built to `web/dist/` and embedded with
`rust-embed`; a `build.rs` runs `zero build` so `cargo build` stays one command
and *fails loudly* if `zero`/the build is missing (resolved decision #1 —
*MIT + Node-free*, the binary still ships everything). `/_/` serves the embedded
SPA; `/_/api/*` is a small hand-written JSON/SSE seam that reuses `Store`/`Db`/
`DataDir` **directly** (no self-HTTP, no SigV4 in the UI — *dev tool first*, keep
the seam thin and swappable).

The live log is the wedge and the one non-trivial mechanism. The event bus is a
`tokio::sync::broadcast` + ring buffer (per CONCEPT). Capture is **two-part,
joined through request extensions**: an outer HTTP layer wrapping the `s3s`
service is the *authoritative emitter* (it always runs, times the request, reads
the final status, counts bytes, and parses the S3 `<Code>` from error XML), and
an `S3Access` impl *enriches* the in-flight event with the **resolved `op`
name** (`cx.s3_op().name()`) and auth kind. The join is an `Arc<Mutex<…>>` slot
the wrapper inserts into the hyper request extensions and `S3Access::check`
fills — validated against the `s3s` source (`prepare()` exposes
`extensions: &mut req.extensions`, and the sig check runs *before* the access
check, so bad-signature 403s are emitted by the wrapper with no resolved op,
which is honest). Same event feeds three consumers: SSE, a pretty stdout line,
and `?format=ndjson`.

## Files

- `Cargo.toml` — add `rust-embed`; `serde` (derive) for the event/JSON DTOs
  (`serde_json` already present); `tokio-stream`/`async-stream` for the SSE body.
- `build.rs` — **new.** Run `zero build` (in `web/`) before compile;
  `cargo:rerun-if-changed=web/src`, `web/styles`, `web/index.html`. Hard-error
  with a clear message if `zero` isn't on `PATH`.
- `web/` — **new** `zero init` project (TS + `html` + SCSS design system). Routes
  for the three screens; a small `zero/http` API client and an `EventSource`
  wrapper for the log.
- `.gitignore` — ignore `web/dist/` (built fresh, decision #1) and zero's
  regenerable caches; keep `web/src`, `web/styles`, `web/index.html`,
  `web/zero.toml` committed.
- `src/embed.rs` — **new.** `#[derive(RustEmbed)] #[folder = "web/dist/"]` + a
  helper to serve an embedded asset (content-type, cache header) and the SPA
  fallback.
- `src/events.rs` — **new.** `Event` struct (serde), `EventBus`
  (`broadcast::Sender` + `Mutex<VecDeque>` ring buffer of ~1000, monotonic id),
  `subscribe()` returning buffered-then-live, and the pretty stdout formatter.
- `src/access_log.rs` — **new.** `S3Access` impl holding the request-extension
  slot type; records resolved `op` + auth into the slot during `check`.
- `src/http.rs` — replace the `/_/` 501 with: SPA/asset serving, `/_/api/*`
  dispatch, and the **logging wrapper** around `S3Service` that emits events.
  Wire the `EventBus` and `set_access(...)` into `build_router`.
- `src/api/mod.rs` (+ `buckets.rs`, `objects.rs`, `search.rs`, `presign.rs`,
  `events.rs`, `health.rs`) — **new.** The JSON/SSE handlers.
- `src/presign.rs` — **new.** Server-side SigV4 presigned-URL generation via
  `s3s::sig_v4` public fns (`create_presigned_canonical_request`,
  `create_string_to_sign`, `calculate_signature`, `AmzDate`).
- `src/db.rs` — add `search_objects(bucket: Option<&str>, term, limit)`
  (`key LIKE '%term%'`, `WITHOUT ROWID` scan), and cheap `count_objects()` /
  `count_buckets()` for `health`.
- `src/cli.rs` — add `--quiet` to `ServeArgs`.
- `src/banner.rs` — unchanged (already prints the `/_/` URL).
- `README.md` — document the UI, `--quiet`, and the presign host-in-signature
  Docker gotcha.

## Risks & unknowns

- **Event correlation slot.** Validated by reading `s3s` 0.14, but confirm
  end-to-end in **A1** (spike) before building on it: the wrapper's
  request-extension `Arc` slot must be visible/fillable in `S3Access::check`.
  Fallback if it isn't: emit op-less events from the wrapper and derive `op` from
  method+path+query (a finite, testable mapping).
- **Bad-signature 403s bypass the access hook** (sig check precedes access
  check). Accepted: the wrapper emits them with status + `error_code` parsed from
  the error XML and a null/`—` op. Matches the spec's "why 403 at a glance"
  criterion (it needs status + code, not a resolved op).
- **Presign generation** with `s3s` primitives is moderate glue; the signed
  `Host` must equal the request host or the URL 403s (CONCEPT's documented sharp
  edge). Spike in **C2**; document, don't rewrite.
- **`zero build` couples `cargo build` to `zero` on `PATH`** (CI installs it via
  `cargo install zero --locked`). `rust-embed` in debug reads `web/dist` from
  disk (nice for `cargo run`); release embeds. If `web/dist` is absent the
  embed derive fails at compile time — the intended loud failure.
- **High-rate SSE → zero table perf** and **mounting the SPA under `/_/`** are
  spec open-question #5. If `zero` lacks a base-path config, an append-optimized
  list, or an `EventSource` helper, the fix may land in `zero` itself (user is
  maintainer). Flag per item in the step where it bites; don't block.
- **`bytes_out`** requires wrapping the response body stream to count; keep it a
  cheap pass-through counter, no buffering (the log must never add latency).

## Progress notes

- **Base-path (open-question #5a) — resolved in-app, no `zero` change.** `zero`
  has no mount-prefix config, and built assets reference `/assets/…` +
  `/.zero/fonts/…` (the built JS has *no* absolute-root refs). Rather than modify
  the separate `zero` framework repo, the SPA is mounted under `/_/` two ways:
  (1) the app registers every route under the `/_/` prefix (zero matches against
  the real `window.location.pathname`, which *is* `/_/…`, so
  `navigate("/_/…")` + link interception work natively); (2) the server rewrites
  the handful of root-absolute asset refs (`/assets/` → `/_/assets/`, `/.zero/` →
  `/_/.zero/`) **at serve time** (`src/embed.rs`) for `index.html` and CSS, so
  embedded assets resolve under `/_/`. Self-contained and fully verifiable; if
  base-path support later lands in `zero`, this can be simplified.
- **Rewrite moved from `build.rs` to serve time (bugfix).** Originally `build.rs`
  rewrote the asset refs on disk after `zero build`. That coupled correctness to
  `cargo build`: a UI dev running `zero build` directly (the natural inner-loop)
  regenerated `dist/` in `zero`'s native form (`/assets/…`), and the debug
  `rust-embed` reads `dist/` from disk — so the app's JS/CSS 403'd through the S3
  service. The prefixing now lives in `src/embed.rs` (`serve_index` /
  `serve_embedded`), applied to the always-native on-disk build. `build.rs` only
  runs `zero build`. Robust to either build path, and non-doubling (only ever
  sees un-prefixed input). Covered by `embed.rs` unit tests + the `web_ui.rs`
  asset-load integration test.
- **`zero.toml` lives at the repo root, `[project] root = "web"`.** `zero init`
  is designed to run from the *project root*: it writes `zero.toml` there and
  scaffolds the app under the configured `root` (`web/`), giving `web/src`,
  `web/index.html`, `web/dist/` with **no** nesting. Every `zero` command
  (`build`/`test`/`dev`/`lint`) — and `build.rs` — then runs from the repo root
  with no `cd`. (An earlier draft invoked `init` *inside* `web/`, which produced
  `web/zero.toml` with `root = "web"` → `web/web/`, then hand-flattened it to
  `root = "."`; that was a misuse of `init`, not a framework quirk.)
- **`web/.zero/` is git-ignored (zero's default), regenerated on demand.**
  `.zero/` is a framework-owned cache (type defs, SCSS tokens, fonts, component
  lib), fully reproducible from `zero.toml` + the installed CLI via
  `zero update -y` (verified: deleting it and re-running regenerates it
  non-interactively). zero's generated `web/.gitignore` ignores it, and so do
  we. `build.rs` regenerates it with `zero update -y` when absent, so a fresh
  `git clone` + `cargo build` self-heals. `web/.gitignore` ignores `.zero/`,
  `dist/`, `coverage/`, `mutation/`.
- **UI built once, after the JSON seam is complete.** The plan interleaves each
  group's UI box with its API boxes, but the UI is a single `zero` SPA with
  shared chrome and three screens. To avoid scaffolding the SPA three times and
  to build every screen against a finished, curl-tested seam, the *backend*
  API boxes for Groups A/B/C are implemented first, then the SPA screens
  (live-log UI, browser UI, object-detail UI) are built together at the end of
  the groups. Each backend box stays independently observable via curl/SDK; the
  UI boxes remain human-observable in the browser. Box order within the plan is
  otherwise unchanged.
- **Boxes 0.2 + 0.3 land together.** `rust-embed`'s `#[folder = "web/dist/"]`
  requires the folder to exist at compile time, and `web/dist/` is git-ignored
  (decision #1), so `build.rs` *must* run `zero build` before the embed derive
  compiles. Embed-serve and build.rs are therefore implemented as one change.
- **`POST /_/api/buckets` added (create-bucket) — divergence from the specced
  read-only seam.** The seam had no way to *create* a bucket, so the browser
  could only be tested against buckets made by an external S3 client — a dead end
  for a self-contained dev tool (surfaced during UI testing). Added `POST
  /_/api/buckets {"name"}` behind the seam: it reuses the Phase-1/2 create path
  exactly (validate name → **dir first, then row** → `409` on duplicate, `400` on
  a bad name), so it is not a *new* S3 verb, just the existing one exposed to the
  UI. Confirmed with the user before adding. UI: a "+ New bucket" affordance in
  the buckets column. Covered by `tests/web_api.rs` (create / invalid / conflict)
  + a `browser.test.ts` render test. Also fixed during this pass: an inline
  favicon (stops `/favicon.ico` 403-ing through the S3 handler and polluting the
  log), dropped the 5 s health poll (load once + refresh after UI mutations), and
  reworked the shell CSS onto zero primitives (`split`/`flank`/`stack
  justify-between`) so no `margin`/fixed-height layout remains.

## Steps

Each box ≈ one small commit moving an observable behavior. Check only when the
outcome is real.

<!-- Progress note (presign / C2): the plan assumed s3s's SigV4 helpers
(`create_presigned_canonical_request`, `create_string_to_sign`,
`calculate_signature`, `AmzDate`) are public. In s3s 0.14.1 the `sig_v4` and
`http` modules are **private** (`mod sig_v4;`), so `OrderedHeaders` and those
fns are unreachable. `src/presign.rs` therefore reimplements SigV4 query-string
presigning directly (sha2 + hmac), matching s3s's canonicalization
(`UNSIGNED-PAYLOAD`, AWS URI-encoding with uppercase hex, `host`-only signed
headers) byte-for-byte so the server's own verifier accepts the URL. Verified
end-to-end in C2 by curling the minted URL with no credentials. -->


### Group 0 — plumbing (serve the app + seam skeleton)

- [x] **Scaffold the `zero` app.** `zero init` into `web/`; `zero build` produces
      `web/dist/index.html`. Observable: `zero build` exits 0 and `web/dist/`
      exists.
- [x] **Embed + serve at `/_/`.** Add `rust-embed` + `src/embed.rs`; replace the
      `/_/` 501 so `GET /_/` returns the embedded `index.html` and `GET
      /_/assets/…` returns assets with correct content-types. Observable:
      `curl -s localhost:PORT/_/ | grep -q '<div id="app"'` and the old "coming
      in Phase 5" body is gone.
- [x] **`build.rs` runs `zero build`.** `cargo build` regenerates `web/dist` and
      errors clearly without `zero`. Observable: touch a `web/src` file →
      `cargo build` re-embeds it.
- [x] **SPA fallback + API 404 split.** Non-API, non-asset `/_/…` → `index.html`;
      unknown `/_/api/…` → `404` JSON `{"error":{…}}`. Observable:
      `curl /_/buckets/x` → HTML `200`; `curl /_/api/nope` → JSON `404`.
- [x] **`GET /_/api/health`.** Returns `{status,version,uptime_s,data_dir,
      endpoint,region,bucket_count,object_count}` (adds `Db::count_*`).
      Observable: `curl /_/api/health | jq .status` → `"ok"`.

### Group A — live request log (the wedge)

- [x] **A1 (spike): event bus + correlated capture.** Add `src/events.rs`
      (`Event`, `EventBus`, ring buffer) and `src/access_log.rs` (`S3Access`).
      Wrap `S3Service` in the logging layer; `set_access(...)` in `build_router`.
      Observable: `RUST_LOG` off, run `aws s3 cp f s3://b/k`; the process logs one
      captured `Event` (temporarily via `tracing`) with resolved
      `op="PutObject"`, `status=200`, non-zero `duration_ms`, `bytes_in`. Proves
      the extension-slot join before any UI.
- [x] **Pretty stdout line + `--quiet`.** Each event prints an aligned
      `PUT  b/k  200  12ms  2.4MB` line; `--quiet` suppresses it. Observable:
      `aws s3 cp` prints the line; with `--quiet` it doesn't.
- [x] **`GET /_/api/events` (SSE).** Stream `text/event-stream`, each frame
      `id:`+`data:`(JSON event); new subscribers replay the ring buffer, honor
      `Last-Event-ID`; lagged consumers get a `dropped` marker. Observable:
      `curl -N /_/api/events` held open, then `aws s3 cp f s3://b/k` in another
      shell → a `data:` frame with that op and a monotonic `id:`.
- [x] **`?format=ndjson`.** Same events, one JSON object per line, no SSE
      framing. Observable: `curl -N '/_/api/events?format=ndjson' | jq .op`
      emits one op per request.
- [x] **`error_code` on failures.** Wrapper parses `<Code>` from S3 error XML for
      status ≥ 400. Observable: a wrong-secret client → event `status=403`,
      `error_code="SignatureDoesNotMatch"`.
- [x] **Live-log UI (home screen).** zero route: subscribe via `EventSource`,
      render the table (columns per the mockup: TIME/METHOD/OPERATION/BUCKET·KEY/
      STATUS/DUR/BYTES), color by status class, batch inserts per animation
      frame. Observable (human): open `/_/`, run `aws s3 cp` → a row appears live.
- [x] **Log UX: filter, pause, auto-scroll, expand.** Filter-as-you-type; Pause
      shows an "N new" badge; auto-scroll stops on scroll-up; row click expands
      full fields (op/auth/error_code/bytes). Observable (human): each behaves as
      described against a live stream.

### Group B — bucket browser

- [x] **`GET /_/api/buckets`.** Lists buckets from `Db::list_buckets` with
      per-bucket count/size for the mockup's bucket column. Observable:
      `aws s3 mb s3://demo` then `curl /_/api/buckets | jq '.buckets[].name'`
      shows `demo`.
- [x] **`GET …/objects` (folder view).** Prefix + `delimiter=/` +
      continuation via the Phase 2 listing path → `{common_prefixes,objects,
      next_continuation_token}`. Observable: with `a/1.txt`,`a/2.txt`,`b.txt`,
      `curl '…/objects?delimiter=/'` → `common_prefixes:["a/"]`, objects `[b.txt]`.
- [x] **`GET /_/api/search`.** Flat full-key `LIKE` search, optional `bucket`
      scope, capped with `truncated` (`Db::search_objects`). Observable:
      `curl '…/search?q=report&bucket=demo'` returns only matching keys;
      `q=port` matches `a/report.pdf` (substring, not prefix).
- [x] **`GET …/objects/{key}?content`.** Streams bytes with stored content-type,
      honoring `Range` (reuse Phase 1 read path). Observable:
      `curl '…/content'` of an uploaded file returns the exact bytes; a `Range`
      request returns `206` + the slice.
- [x] **`PUT …/objects/{key}` (upload) + `DELETE …/objects/{key}`.** Upload
      streams through the Phase 1 `.tmp`→fsync→rename→row path; delete is
      row-first-then-unlink. **Not** logged as S3 traffic (decision #2).
      Observable: `curl -T f …/objects/x.bin` → `cmp s3data/buckets/demo/x.bin f`
      clean; `curl -X DELETE …/objects/x.bin` → file gone; neither appears in
      `/_/api/events`.
- [x] **Bucket-browser UI.** zero route: bucket column, breadcrumb + folder/
      object table, per-row download (`?content`) and delete, empty state
      ("No objects yet. Drop files to upload to `<bucket>/`"). Observable
      (human): navigate folders; breadcrumbs track the prefix.
- [x] **Drag-drop upload UI.** Drop zone → `PUT …/objects/{prefix+name}`; row
      appears on refresh. Observable (human): drop a file →
      `cat s3data/buckets/demo/<prefix><name>` shows the bytes.
- [x] **Search UI + "all buckets" toggle.** Search box swaps folder view for a
      flat match list (term highlighted, "N matches"); toggle widens to all
      buckets (rows show `bucket/key`). Observable (human): typing `report`
      narrows to matches; clearing restores folder view.
- [x] **`POST /_/api/buckets` + "New bucket" UI (divergence — see Progress
      notes).** The specced seam was read-only, leaving no way to create a bucket
      from the UI; added the existing Phase-1/2 CreateBucket verb behind the seam
      (dir-first-then-row, `400` invalid name, `409` duplicate) + a "+ New bucket"
      affordance in the buckets column. Observable: `curl -X POST /_/api/buckets
      -d '{"name":"demo"}'` → `200`, bucket visible to `aws s3 ls`; in the browser
      the button creates and selects it. Covered by `tests/web_api.rs` (create /
      invalid / conflict) + a `browser.test.ts` render test.

### Group C — object detail

- [x] **`GET …/objects/{key}` (meta).** `{key,size,etag,content_type,
      last_modified,storage_class:"STANDARD",metadata}` or `404 NoSuchKey`.
      Observable: `curl …/objects/x.png | jq .etag` equals
      `aws s3api head-object` ETag.
- [x] **C2 (spike): `POST /_/api/presign`.** `src/presign.rs` mints a query-string
      SigV4 URL for `{method,bucket,key,expires_in_s}` via `s3s::sig_v4` fns,
      signing with the configured creds and the **request host**. Observable:
      `curl -X POST /_/api/presign -d '{"method":"GET",...}'` returns a URL;
      `curl` of that URL (no creds) returns the object bytes `200`.
- [x] **Object-detail UI.** zero sub-view of the browser: preview pane
      (image/text/JSON via `?content`, else download), metadata table, user-
      metadata rows. Observable (human): open a PNG → inline preview + metadata
      matching `head-object`.
- [x] **Presign button + expiry picker UI.** GET/PUT toggle, EXPIRES IN dropdown,
      Generate → URL in a copy field. Observable (human): pick 1h, Generate, open
      the URL in a creds-less tab → object loads; a 1s-expiry URL opened later →
      `403`.

### Docs

- [x] **Docs** — update `README.md`: the web UI at `/_/` and what it shows
      (live log, browser, presign), the `--quiet` flag, that `--bind 0.0.0.0`
      exposes the UI, and the presigned-URL **host-in-signature Docker gotcha**
      (CONCEPT sharp edge). Note the `zero`/`cargo build` toolchain requirement
      for contributors. (Added a "Web UI (Phase 5)" section + `--quiet` flag row;
      removed the stale "501 placeholder" line.)

## Acceptance

Mirrors the spec. `/implement` isn't done until every box passes by driving the
named client/observer.

- [x] **UI loads from the binary, offline.** No network/Node; `GET /_/` returns
      the built `index.html` `200` and its assets load `200`; the 501 body is
      gone.
- [x] **SPA fallback.** `GET /_/buckets/foo` → `index.html` `200`;
      `GET /_/api/nonesuch` → `404` JSON.
- [x] **`health`.** `curl /_/api/health` → `200` `{"status":"ok",…}`.
- [x] **`buckets` mirrors S3.** After `aws s3 mb s3://demo`, `curl /_/api/buckets`
      lists `demo`.
- [x] **`objects` folder view.** `delimiter=/` → `common_prefixes:["a/"]` +
      `b.txt` (rides Phase 2 listing).
- [x] **UI upload lands a real file.** `PUT …/objects/x.bin` → `cmp
      s3data/buckets/demo/x.bin` clean; a later authed `aws s3 cp s3://demo/x.bin
      -` returns the same bytes.
- [x] **UI delete removes the file.** `DELETE …/objects/x.bin` →
      `Path("s3data/buckets/demo/x.bin")` gone and not listed.
- [x] **THE headline — multipart decomposes live.** With the log open in a
      browser, a boto3 100MB `upload_file` shows `CreateMultipartUpload` +
      N×`UploadPart` + `CompleteMultipartUpload` in real time, each with resolved
      `op`, `200`, byte counts — no refresh, no S3 client in the UI.
- [x] **…then browse and download the result.** Same session: navigate to the
      object and download via the UI; bytes match the original.
- [x] **SSE emits live over `curl`.** `curl -N /_/api/events` + `aws s3 cp` →
      an SSE frame for that op with a monotonic `id:`.
- [x] **`error_code` visible on 403.** A wrong-secret/`--no-sign-request` request
      → event `status:403`, `error_code:"SignatureDoesNotMatch"`/`AccessDenied`.
- [x] **ndjson for pipes.** `curl -N '/_/api/events?format=ndjson' | jq .op`
      emits one op per request.
- [x] **Replay after reconnect.** `curl -N -H 'Last-Event-ID: <n>' /_/api/events`
      replays events after `<n>` from the ring buffer.
- [x] **Pretty stdout line.** `aws s3 cp` prints an aligned `… 200 …ms …B` line;
      `--quiet` suppresses it.
- [x] **Drag-drop upload (human).** Dropping a file uploads it; it lists and
      `cat s3data/buckets/demo/<prefix><name>` shows the bytes.
- [x] **Folder navigation (human).** Clicking a folder drills in; breadcrumbs
      navigate back up.
- [x] **Delete from the browser (human).** Deleting a row removes it from the
      list and from disk.
- [x] **Scoped key search.** `curl '…/search?q=report&bucket=demo'` returns
      exactly the `report` keys (flat, no rollup); the browser shows them.
- [x] **Global "all buckets" search.** `curl '…/search?q=report'` returns matches
      across buckets, each tagged with `bucket`; the toggle shows both.
- [x] **Search is substring, not prefix.** `q=port` matches `a/report.pdf`.
- [x] **Inline preview (human).** A PNG previews inline; a `.json` renders as
      text; metadata matches `aws s3api head-object` (incl. `storage-class
      STANDARD`).
- [x] **Presign button resolves (human + curl).** Choosing an expiry + Generate
      yields a URL that returns the bytes `200` creds-less; a 1s-expiry URL used
      after it lapses → `403`.
