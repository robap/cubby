# Web UI — the S3 debugger — spec

**Status:** done · **Roadmap:** Phase 5 · **Slug:** 05-web-ui

## Why

Phases 1–4 make cubby a *correct* S3 stand-in. This phase makes it the thing
someone actually **chooses**: a live, in-browser **S3 debugger**. Everything
before this is table stakes — every dev-mode object store can round-trip a PUT.
The differentiator is watching S3 traffic happen: seeing one `s3.upload()`
decompose into `CreateMultipartUpload` + N×`UploadPart` + `CompleteMultipartUpload`
in real time, and knowing at a glance *why a request 403'd*.

The UI is built with **zero** (dogfooding; keeps the repo Node-free) and served
as static assets embedded in the single binary at `/_/`. Per CONCEPT it speaks
a **thin, boring JSON seam** under `/_/api/` (~8 endpoints, no SigV4 in the UI)
so the framework choice stays invisible and swappable.

North stars served:
- **Dev tool first** — the live request log *is* the product's reason to exist.
  Log readability and test ergonomics are the feature, not a nicety.
- **Starts in milliseconds, zero config** — the UI ships inside the binary; no
  separate process, no `npm`, no build step for the user. `./cubby serve` and
  the UI is at `/_/`.
- **MIT + Node-free** — zero (Rust binary) + `rust-embed`; the whole repo still
  builds with a Rust toolchain alone.
- **Compatibility is proven, not claimed** — the acceptance headline is a human
  driving a *real* boto3 multipart upload and watching it decompose live.

## In scope

Ordered by the ROADMAP's "live log first, it's the wedge" sequencing. The plan
should land these as three separable commit groups (A → B → C) behind the same
shared plumbing (0).

**0. Plumbing — serving the app + the JSON seam.**
- Replace the `/_/` 501 placeholder (`src/http.rs`) with a handler that serves
  the embedded zero build (`rust-embed`) and routes `/_/api/*` to the JSON API.
- SPA behavior: `GET /_/` and any non-API `/_/…` path that isn't a real asset →
  serve `index.html` (client-side routing); real assets served with correct
  content-types and a long cache header.
- The `/_/api/` handlers reuse the existing `Store`/`Db`/`DataDir` directly (no
  self-HTTP-to-S3, no SigV4 in the UI path).
- Dev workflow: `zero dev` runs the UI at its own port and **proxies** unmatched
  requests (incl. `/_/api/*` and `/_/api/events`) to a running `cubby serve`, so
  the UI is hot-reloadable against a live backend.

**A. Live request log (home screen) — the wedge.**
- An in-process event bus: `tokio::sync::broadcast` + a ~1,000-event ring
  buffer, populated by an interceptor in the HTTP layer that wraps the `s3s`
  service and records one event per S3 request.
- Event struct (per CONCEPT): `{id, ts, method, op, bucket, key, status,
  duration_ms, bytes_in, bytes_out, auth, error_code}`. `op` is the **resolved
  S3 operation** (e.g. `CreateMultipartUpload`), not just the HTTP method.
  `error_code` is the S3 error code on failures (`NoSuchKey`,
  `SignatureDoesNotMatch`, …). No headers/signatures in the stream.
- Three consumers of the same event: SSE (`GET /_/api/events`), a pretty
  **stdout** line (the banner-style `PUT uploads/… 200 12ms 2.4MB`), and
  `GET /_/api/events?format=ndjson` (one JSON object per line, for `jq`/tests).
- SSE served over plain HTTP on the existing port: `EventStream`/`text/event-stream`,
  each event carries an `id:` so `EventSource` auto-reconnect replays via
  `Last-Event-ID` from the ring buffer. Lagged consumers get a "dropped N"
  marker, never backpressure.
- UI: a live-updating table — color by status class (2xx/3xx/4xx/5xx),
  filter-as-you-type, **pause** with an "N new" badge, auto-scroll that stops
  when the user scrolls up, click a row to expand its full field set. Batches
  DOM inserts per animation frame at high event rates.

**B. Bucket browser.**
- Bucket list; folder-style navigation within a bucket using
  prefix + `/` delimiter (CommonPrefixes render as folders), with breadcrumbs.
- Upload via drag-and-drop (and a file picker) into the current prefix.
- Per-object row actions: download, delete. Empty/large states handled.
- **Object search** — a search box that substring-matches **full keys** and
  renders a **flat** result list (folders un-rolled, full keys shown). Scoped to
  the current bucket by default, with an **"all buckets"** toggle for a global
  cross-bucket search. UI-only convenience (see *Behavior* — S3 itself has no
  substring search); distinct from the live-log's own event filter.

**C. Object detail.**
- Metadata panel: key, size, content-type, ETag, last-modified, user metadata.
- Inline preview for images, text, and JSON (bytes streamed through the API,
  size-capped); other types show a download affordance only.
- **Presigned-URL button** with an **expiry picker** (e.g. 5 min / 1 h / 24 h /
  7 days). The server mints the URL (SigV4 stays server-side); the UI shows the
  URL with copy-to-clipboard.

**JSON seam (~9 endpoints under `/_/api/`).** Exact shapes in *Behavior*:
`health`, `buckets` (list), `objects` (list w/ prefix+delimiter+continuation),
`search` (flat substring key search, optional bucket scope), object `meta`,
object `content` (stream/preview/download), `upload`, `delete`, `presign`,
`events` (SSE + ndjson). (Object meta/content/upload/delete share the
`/_/api/buckets/{bucket}/objects/{key}` path by method.)

## Out of scope

- **Any S3 API surface changes.** No new S3 verbs; the seam reuses Phases 1–4.
- **CORS / `--cors`** — a v0.2 flag (CONCEPT/ROADMAP). The presign button will
  tempt people to `fetch()` the URL cross-origin from a frontend; that's gated
  in v0.2. A browser `GET`/`PUT` directly to a presigned URL still works today.
- **Auth on `/_/` and `/_/api/`.** The UI/API are **unauthenticated**, sharing
  the same trust boundary as the data directory itself (a dev tool bound to
  `127.0.0.1`). No login, no SigV4 in the UI. When `--bind 0.0.0.0` exposes the
  server, it exposes the UI too — documented, not gated. (See open questions.)
- **Editing metadata / renaming / bulk operations in the UI.** Read + upload +
  delete + presign only. Metadata edits are the S3 `CopyObject REPLACE` idiom
  (Phase 4), not a UI feature here.
- **Persisting the live log.** In-memory ring buffer only; resets on restart
  (correct for a dev log). No search over history beyond the buffer.
- **Headers/signatures in the event stream**, request/response body capture, or
  a "replay this request" button. People screenshot logs; keep secrets out.
- **The multipart "highlight all events sharing this `upload_id`" flourish** —
  CONCEPT marks it post-MVP. Grouping/filtering by `upload_id` is a *nice-to-have*
  here; the acceptance headline only requires the three ops be visibly present
  and attributable to one upload (see criteria).
- **Auth/error analytics, charts, metrics dashboards.** The log is a stream, not
  a BI tool.
- **Virtual-host addressing, multi-instance, remote buckets.** One local
  instance, path-style, as everywhere else.

## Behavior

### Serving & routing
- `GET /_/` → the embedded `index.html`. `GET /_/assets/…` (or whatever zero
  emits) → the corresponding embedded asset with its content-type and a
  cache-friendly header. A non-API, non-asset `/_/…` path → `index.html`
  (SPA fallback) so client-side routes deep-link.
- `/_/api/*` is always JSON (or SSE) and never falls back to `index.html`; an
  unknown API path → `404` JSON.
- Underscore-prefixed bucket names are illegal in S3, so `/_/` cannot collide
  with any S3 path (unchanged from CONCEPT's routing rule).
- **Errors** from the seam use a small consistent JSON envelope
  (`{"error":{"code","message"}}`) with an appropriate HTTP status.

### JSON API contract (the thin seam)
All under `/_/api/`. Keys are percent-safe in the path; list responses carry raw
S3 keys in the JSON body.

- `GET health` → `{"status":"ok","version":"<semver>","uptime_s":<n>,
  "data_dir":"~/.cubby/local.store","endpoint":"http://127.0.0.1:9000",
  "region":"us-east-1","bucket_count":<n>,"object_count":<n>}`. The extra fields
  feed the **top bar** (data-dir, endpoint, health dot) and the left-nav
  **footer** (`N buckets · M objects · region`) seen in the mockups. `region` is
  the accepted-and-ignored default (display only). (Counts may instead be
  derived from `buckets`; keep them wherever one cheap query serves the chrome.)
- `GET buckets` → `{"buckets":[{"name","created_at"}, …]}` (from SQLite).
- `GET buckets/{bucket}/objects?prefix=&delimiter=/&continuation-token=&max-keys=`
  → `{"prefix","delimiter","common_prefixes":["a/","b/"],
  "objects":[{"key","size","etag","last_modified"}, …],
  "next_continuation_token": <string|null>}`. Served from the Phase 2 listing
  path; folder view uses `delimiter=/`.
- `GET search?q=<term>&bucket=<optional>&max-keys=` → **flat** substring key
  search: `{"q","bucket": <name|null>,
  "results":[{"bucket","key","size","etag","last_modified"}, …],
  "truncated": <bool>}`. With `bucket` set → scoped to that bucket; omitted →
  global across all buckets (hence `bucket` on each result). Matches the **full
  key** (`key LIKE '%term%'`), never rolls up folders, and is capped
  (`max-keys`, default e.g. 1000) with `truncated` signaling more. This is a
  **UI/seam convenience, not an S3 capability** (S3 lists by prefix only); a
  leading-wildcard `LIKE` can't use the clustered `(bucket,key)` index, so it is
  a full scan — acceptable for a dev tool's object counts.
- `GET buckets/{bucket}/objects/{key}` (meta) →
  `{"key","size","etag","content_type","last_modified","storage_class":"STANDARD",
  "metadata":{…}}` or `404` `NoSuchKey`. `storage_class` is the constant
  `STANDARD` (storage classes are a CONCEPT non-goal; the object-detail panel
  displays it for parity with S3, it is never variable).
- `GET buckets/{bucket}/objects/{key}?content` (or `…/content`) → streams the
  object bytes with its stored content-type, honoring `Range`; used for inline
  preview (size-capped by the UI) and download.
- `PUT buckets/{bucket}/objects/{key}` (upload) → streams the request body
  through the **Phase 1 write path** (`.tmp/` → fsync → rename → row); returns
  `{"key","size","etag"}`. Result is a real browsable file on disk.
- `DELETE buckets/{bucket}/objects/{key}` → Phase 1 delete (row first, then
  unlink); `204`/`200`. Idempotent.
- `POST presign` `{"method":"GET"|"PUT","bucket","key","expires_in_s"}` →
  `{"url":"http://…?X-Amz-Signature=…","expires_at"}`. The server signs with the
  configured credentials (the same SigV4 `s3s`/`SimpleAuth` validates). Path-style
  host matches the request's host (the signed host = what the browser will hit),
  so the returned URL resolves against this instance.
- `GET events` → SSE (below). `GET events?format=ndjson` → the same events as
  newline-delimited JSON (no SSE framing) for piping to `jq`/test harnesses.

### Live event bus & SSE
- One event is recorded per S3 request by an interceptor wrapping the `s3s`
  service in the HTTP layer — capturing method, the resolved `op`, bucket/key,
  final status, wall-clock `duration_ms`, `bytes_in`/`bytes_out`, `auth`
  (header / presigned / anonymous), and `error_code` on failure.
- **UI/API-originated mutations** (upload/delete via `/_/api/`) are **not** S3
  wire requests. Default: they do **not** appear in the live log — the log is an
  honest mirror of *client* S3 traffic, and polluting it with the operator's own
  clicks would undermine the debugger story. (See open questions — alternative
  is to tag them `auth:"ui"`.)
- `broadcast` channel + ~1,000-event ring buffer. New SSE subscribers first
  replay the buffer (or from `Last-Event-ID`), then stream live. A slow consumer
  that lags the channel receives a synthetic `{dropped:N}` marker rather than
  stalling the server.
- Same struct feeds stdout: a compact aligned line per request
  (`PUT  uploads/photos/cat.jpg   200   12ms   2.4MB`), on by default,
  suppressed by `--quiet`.

### Screen behavior
- **Live log (home):** newest at the bottom, auto-scrolling; scrolling up pauses
  auto-scroll and shows an "N new" badge that resumes on click. A text filter
  narrows visible rows live. Status colored by class. Clicking a row expands the
  full field set (op, auth, error_code, bytes). Reconnects transparently after a
  network blip (`EventSource`), replaying missed events from the buffer.
- **Bucket browser:** breadcrumb reflects the current `prefix`; folders
  (CommonPrefixes) are clickable, objects show size/last-modified. Drag-drop or
  pick files → each uploads to `current-prefix + filename` via `PUT` and appears
  on refresh; per-row download and delete. A **search box** switches the view
  from folder-browse to a **flat** match list as the user types (querying
  `/_/api/search`): full-key substring matches, folders un-rolled, each row a
  clickable full key. An **"all buckets"** toggle widens the scope from the
  current bucket to every bucket (results then show `bucket/key`). Clearing the
  box returns to folder-browse. Distinct from the live-log's event filter.
- **Object detail:** shows metadata; previews images/text/JSON inline (capped);
  presign button + expiry picker returns a URL that opens the object in a new
  tab.

### Build & embedding
- UI source lives in a `web/` zero project (`zero build` → `web/dist/`), embedded
  via `rust-embed` at compile time. The user's `./cubby serve` needs no Node and
  no network.
- Whether `web/dist/` is committed or built fresh in CI
  (`cargo install zero --locked` + `zero build`) is the ROADMAP's open decision —
  see below.

## Acceptance criteria

Observers are: a **human in a browser** driving the UI, **`curl`** against the
`/_/api/` seam (JSON/SSE assertions), **real S3 clients** (boto3, AWS CLI)
generating the traffic the log mirrors, and **filesystem** checks for the
"real file" guarantees. Each becomes a plan checkbox.

### Plumbing & seam
- [ ] **The UI loads from the binary, offline.** With no network and no Node,
      `./cubby serve ./s3data` then `GET http://127.0.0.1:PORT/_/` returns the
      built `index.html` (`200`, `text/html`), and its referenced JS/CSS assets
      load `200` — proving `rust-embed` embedding. The old
      "coming in Phase 5" 501 body is gone.
- [ ] **SPA fallback.** `GET /_/buckets/foo` (a client-side route, not an asset)
      returns `index.html` `200`; `GET /_/api/nonesuch` returns `404` JSON
      (never HTML).
- [ ] **`health`.** `curl /_/api/health` → `200` `{"status":"ok",…}`.
- [ ] **`buckets` mirrors S3.** After `aws s3 mb s3://demo`,
      `curl /_/api/buckets` lists `demo` with its `created_at`.
- [ ] **`objects` folder view.** With `a/1.txt`, `a/2.txt`, `b.txt` put,
      `curl '/_/api/buckets/demo/objects?delimiter=/'` returns `common_prefixes`
      `["a/"]` and `objects` containing `b.txt` (proves it rides the Phase 2
      listing path).
- [ ] **UI upload lands a real file.** A `PUT /_/api/buckets/demo/objects/x.bin`
      of a body returns `{"etag",…}` and
      `cmp s3data/buckets/demo/x.bin` against the sent bytes is clean; a
      subsequent authed `aws s3 cp s3://demo/x.bin -` returns the same bytes
      (the S3 side sees the UI-written object).
- [ ] **UI delete removes the file.** `DELETE /_/api/buckets/demo/objects/x.bin`
      → `Path("s3data/buckets/demo/x.bin")` is gone and
      `curl /_/api/buckets/demo/objects` no longer lists it.

### Live log (headline)
- [ ] **THE headline — human watches a multipart upload decompose.** With the
      live-log screen open in a browser, a human runs a **boto3 100MB
      `upload_file`** (forces multipart). The screen shows, in real time, a
      `CreateMultipartUpload`, multiple `UploadPart`, and a
      `CompleteMultipartUpload` event — each with the **resolved `op`**, `200`
      status, byte counts — all attributable to the one upload. No refresh, no
      S3 client involved in the UI.
- [ ] **…then browse and download the result.** In the same session the human
      navigates the bucket browser to the uploaded object and downloads it via
      the UI; the downloaded bytes match the original (ROADMAP's end-to-end
      "no S3 client" assertion).
- [ ] **SSE emits live over `curl`.** `curl -N /_/api/events` held open, then
      `aws s3 cp file s3://demo/k` in another shell → an SSE `data:` frame for
      that `PutObject` (or multipart ops) appears on the stream with a
      monotonic `id:`.
- [ ] **`error_code` is visible on failures.** A request that 403s
      (e.g. `aws --no-sign-request s3 ls s3://demo` or a wrong-secret client)
      produces a log event with `status:403` and
      `error_code:"SignatureDoesNotMatch"`/`AccessDenied` — the "why 403 at a
      glance" promise.
- [ ] **ndjson for pipes.** `curl -N '/_/api/events?format=ndjson' | jq .op`
      emits one resolved-op string per line as S3 requests arrive (no SSE
      framing) — the test-harness/`jq` consumer.
- [ ] **Replay after reconnect.** Events produced while an `EventSource` is
      briefly disconnected are re-delivered on reconnect via `Last-Event-ID`
      from the ring buffer (observable: a `curl -N -H 'Last-Event-ID: <n>'
      /_/api/events` replays events after `<n>`).
- [ ] **Pretty stdout line.** Running the above `aws s3 cp` prints an aligned
      `PUT … 200 …ms …B` line to cubby's stdout; `--quiet` suppresses it.

### Bucket browser & object detail
- [ ] **Drag-drop upload (human).** Dropping a file onto a prefix in the browser
      uploads it; it appears in the listing and `cat s3data/buckets/demo/<prefix><name>`
      shows the real bytes.
- [ ] **Folder navigation (human).** Clicking a folder (CommonPrefix) drills in;
      breadcrumbs reflect the prefix and navigate back up.
- [ ] **Scoped key search (curl + human).** With `a/report.pdf`,
      `logs/report-2.txt`, `photos/cat.jpg` in bucket `demo`,
      `curl '/_/api/search?q=report&bucket=demo'` returns exactly the two
      `report` keys as a flat list (full keys, no folder rollup) and omits
      `cat.jpg`; in the browser, typing `report` replaces the folder view with
      those two rows.
- [ ] **Global "all buckets" search (curl + human).** With `report.txt` in
      bucket `demo` and `report.csv` in bucket `other`,
      `curl '/_/api/search?q=report'` (no `bucket`) returns both, each tagged
      with its `bucket`; toggling "all buckets" in the browser shows both across
      buckets.
- [ ] **Search is substring on the full key, not prefix.** `q=port` matches
      `a/report.pdf` (mid-key substring) — proving it is not the S3
      prefix-only semantics.
- [ ] **Delete from the browser (human).** Deleting an object from a row removes
      it from the listing and from disk.
- [ ] **Inline preview (human).** Opening an uploaded PNG shows it inline; an
      uploaded `.json` renders as text; metadata (size, content-type, ETag)
      matches `aws s3api head-object`.
- [ ] **Presign button resolves (human + curl).** On object detail, choosing an
      expiry and clicking "presign GET" yields a URL; opening it in a
      **credential-less** tab (or `curl`ing it) returns the object bytes `200`.
      A URL minted with a 1-second expiry, fetched after it lapses → `403`
      (matches Phase 4 presigned semantics).

## Design mockup prompt

Paste the following into the Claude design tool to generate a UI mockup. It is
self-contained; adjust palette/labels to taste. (The real UI is built in `zero`,
whose design system already ships light/dark tokens and the Geist / Geist Mono
typefaces — the mockup should evoke that, not a generic dashboard.)

> **Design a web UI for "cubby", a local-development S3-compatible object store —
> think "the SQLite of S3". The UI is a developer debugging tool, not a
> consumer dashboard: dense, fast, terminal-adjacent, monospace for keys and log
> rows. Ship both a light and a dark theme (dark is the hero). Use a modern
> geometric sans (Geist) for chrome and a monospace (Geist Mono) for keys,
> ETags, sizes, and the log. Accent color: a single confident hue (electric
> indigo/cyan). Generous but compact spacing; a persistent top bar with the
> cubby wordmark, the data-dir path, the S3 endpoint URL, and a health dot.
> Left nav switches between three screens.**
>
> **Screen 1 — Live Request Log (the home screen, the star).** A full-height,
> live-streaming table of S3 requests, newest at the bottom, auto-scrolling.
> Columns: time (relative, monospace), METHOD (GET/PUT/POST/DELETE as small
> tags), **OP** (the resolved S3 operation, e.g. `CreateMultipartUpload`,
> `UploadPart`, `PutObject` — this is the teaching column, emphasize it),
> bucket/key (monospace, truncating middle), STATUS (color-coded by class:
> green 2xx, grey 3xx, amber 4xx, red 5xx — show the numeric code), duration
> (ms), and bytes in/out. Above the table: a filter-as-you-type input, a
> **Pause** toggle that turns into an "N new" badge when paused, and an auth/
> status filter. A clicked row expands inline to reveal the full event
> (`op`, `auth`, `error_code`, byte counts). Show a realistic burst: one
> `s3.upload()` decomposed into `CreateMultipartUpload` → several `UploadPart`
> → `CompleteMultipartUpload`, plus a red `403 SignatureDoesNotMatch` row so
> the "why did it 403" value is obvious. Empty state: "Waiting for S3 traffic…".
>
> **Screen 2 — Bucket Browser.** A file-explorer for buckets. Left: bucket list.
> Main: breadcrumb (bucket / prefix path), then a table of folders
> (CommonPrefixes, folder icon) and objects (key, size, last-modified, ETag).
> A drag-and-drop upload zone overlays the table ("Drop files to upload to
> `uploads/photos/`"). Each object row has download and delete actions. At the
> top of the main panel, a **search input** ("Search keys…") with an **"all
> buckets" toggle** beside it; show a searching state where the folder tree is
> replaced by a flat list of full-key matches (the query term highlighted in
> each key, bucket shown as a small prefix tag when "all buckets" is on) and a
> result count. Include a large-file and an empty-bucket state.
>
> **Screen 3 — Object Detail.** A two-column layout. Left: an inline preview
> pane (show an image preview; note text/JSON render as monospace text, other
> types show a download button). Right: a metadata panel (key, size,
> content-type, ETag, last-modified, and a user-metadata key/value list), and a
> **"Generate presigned URL"** control: a method toggle (GET/PUT), an **expiry
> dropdown** (5 min / 1 hour / 24 hours / 7 days), a Generate button, and the
> resulting long URL in a monospace field with a copy button and an
> "expires in 1h" caption.
>
> **Overall:** it should feel like a native dev tool — instant, legible at a
> glance, comfortable next to a terminal. Prioritize the live log; it's the
> reason the tool exists. Deliver the three screens as separate frames plus the
> shared top bar and left nav, in both dark and light themes.

## Design reference (rendered mockups)

The approved mockups live in [`05-web-ui-design/`](05-web-ui-design/). They are
the **visual source of truth** for `/plan` and `/implement`; the zero components
should be built to match. Screens:

- `live-log-dark.png` / `live-log-light.png` — the home screen.
- `bucket-browser-dark.png` — folder view of a bucket.
- `bucket-browser-search.png` — the flat search state (`all buckets` toggle,
  highlighted substring match, "N matches" count).
- `bucket-browser-no-objects-yet.png` — empty-bucket + drop-to-upload state.
- `object-detail-dark.png` / `object-detail-light.png` — metadata, preview,
  presign card.

**What the mockups lock in** (treat as binding UI detail unless the plan flags a
conflict):

- **Top bar:** cubby wordmark + version badge, `DATA-DIR`, `ENDPOINT`, a
  `healthy` status dot, and a light/dark theme toggle. (Feeds off `GET health` —
  see the extended payload above.)
- **Left nav** has exactly **two** destinations under an "INSPECT" heading —
  *Live request log* (with a live-green dot) and *Bucket browser*. **Object
  detail is not a top-level nav item**; it's the sub-view reached by opening an
  object in the browser (breadcrumb back to the containing prefix). "Screen 3"
  in the prompt = this sub-view.
- **Nav footer:** `N buckets · M objects` and `region us-east-1`.
- **Live log table columns, in order:** `TIME` (relative, e.g. `24.35s`),
  `METHOD` (colored tag), `OPERATION` (resolved `op`, emphasized), `BUCKET / KEY`
  (monospace, middle-truncated), `STATUS` (colored pill with numeric code),
  `DUR` (ms; slow ones amber), `BYTES` (with `↑` in / `↓` out arrows). Toolbar:
  filter input ("Filter by op, key, method"), `All status` select, `Any auth`
  select, an `N / N` count, and `Pause`. Multipart shows `UploadPart` with a
  `[part 4/8]` annotation.
- **Bucket browser:** a middle **buckets column** (each row = name + `objects ·
  size`), then the listing pane with `Search keys…` + `all buckets` toggle and a
  `NAME / SIZE / MODIFIED / ETAG` table; folders render with a trailing `/` and
  an "N items" count. Empty state: a box icon, "No objects yet.", "Drop files to
  upload to `<bucket>/`".
- **Object detail:** breadcrumb + `PREVIEW <content-type>` header; a preview
  pane (image thumb / dims for images, monospace for text/JSON, download
  affordance otherwise); an `OBJECT` metadata table (`size` as human + exact
  bytes, `content-type`, `etag`, `last-modified … UTC`, `storage-class
  STANDARD`); a `USER METADATA` table of `x-amz-meta-*` rows; and a **Generate
  presigned URL** card — `METHOD` GET/PUT toggle, `EXPIRES IN` dropdown, Generate
  button, "Time-limited link, no credentials required."

**Deltas the plan should reconcile** (mockups vs. the earlier written spec):

- `GET health` must return more than `status` (data-dir, endpoint, region,
  counts) to dress the top bar and footer — contract updated above.
- Object meta must include a constant `storage_class:"STANDARD"` — updated above.
- Object detail is a *sub-view of the browser*, not a third nav entry — the
  plan's routing (`/_/…` client routes) should reflect a two-item nav.

## Resolved decisions

Confirmed by the user — none of these block planning.

1. **Build fresh; `web/dist/` is git-ignored.** ✅ CI runs
   `cargo install zero --locked` + `zero build`; the binary embeds the fresh
   build via `rust-embed`. Keeps the tree honest and Node-free. Cost: a `zero`
   build step in front of `cargo build`/CI — the plan wires this (e.g. a
   `build.rs` or a documented CI/`Makefile` step) so `cargo build` fails loudly
   if `zero`/the build is missing rather than embedding a stale `dist/`.
2. **UI-initiated uploads/deletes do NOT appear in the live log.** ✅ The log
   mirrors *client* S3 traffic only; the operator's own UI clicks stay out of it.
   (If a future need arises to inspect them, revisit by tagging `auth:"ui"` and
   filtering them out by default — not now.)
3. **`/_/api/` and `/_/` are unauthenticated.** ✅ Same trust boundary as the
   data directory, matching the localhost-dev-tool posture. Document that
   `--bind 0.0.0.0` exposes the UI along with the S3 API. No `--ui-token` in this
   phase.
4. **Preview/download streams through `/_/api/…/content`.** ✅ `GET …/content`
   honors `Range` and is size-capped for inline preview, so the UI needs no
   SigV4 to show bytes. Presigned URLs remain a first-class *explicit* button
   (the expiry-picker control), not the transport for previews.
6. **One spec, three commit groups.** ✅ This file covers the whole phase; the
   plan sequences it as live log → bucket browser → object detail behind shared
   plumbing (consistent with prior multi-feature phases like Phase 4). The live
   log ships first and is independently demoable.

## Open questions

5. **`zero` framework touchpoints — resolved during plan/implementation.** Some
   things the UI needs may not yet exist in `zero`; the user is the maintainer
   and will add them as they surface. Known candidates to settle in the plan:
   (a) a **base-path / mount-prefix** config so the SPA and its assets resolve
   under `/_/` (router + asset URLs); (b) an **`EventSource`/SSE** helper
   alongside `zero/http` for the live log; (c) a **virtualized/append-optimized
   table** for high-rate log rows (CONCEPT flags batching DOM inserts per
   animation frame as a known stress test). The plan should note, per item,
   whether the fix lands in `zero` or in the app.
