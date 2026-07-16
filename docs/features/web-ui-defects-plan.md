# Web UI defects — plan

**Spec:** [web-ui-defects-spec.md](web-ui-defects-spec.md) · **Roadmap:** ad-hoc (Phase 5 follow-up)

## Approach

Four independent defect areas, one plan, sequenced so the shared substrate lands
before its dependents. Three server-side fixes go first (they're small,
self-contained, and unblock the UI): a `.well-known` short-circuit and the root
redirect in `Router::call`, and a server-ring **clear** on the event bus.
Routing (D) lands next because B's jump-to-object rides on it: the browser's
location moves out of `stores/browse.ts` signals and into the **URL** — encoded
as query params on `/_/browser` (`?bucket=&prefix=` for a folder, `?bucket=&object=`
for an object). Keys carry `/` and spaces, so query values percent-encode cleanly
and dodge the folder-vs-object path ambiguity; `route()` exposes a reactive
`query`, so one `syncFromUrl` effect hydrates on cold load *and* on Back/Forward,
and store mutators become thin `navigate()` calls — the URL is the single source
of truth. This directly serves *the filesystem is the API too* (north star 2):
every real object is now linkable and bookmarkable. The rest — time-ago labels,
Clear button, jump link, preview scroll + pretty-print, and the three-state theme
control with `localStorage` persistence — are localized UI changes serving *dev
tool first* (north star 4): a readable, clearable log and a trustworthy browser.

Pure logic (time-ago, JSON/XML pretty-print, browser-detection) goes in
unit-tested `lib/` helpers per the repo's pattern (`log.ts`, `preview.ts`,
`format.ts` already have `.test.ts` siblings). UI acceptance is observed by
rebuilding the committed assets (`zero build` → `web/dist/`) and running the
binary (`cargo run`), since Rust embeds `web/dist/` (CONCEPT principle #5).

## Files

**Server (Rust)**
- `src/http.rs` — `Router::call`: short-circuit `/.well-known/…` (no log, `404`);
  root redirect for browser `GET /` (`302 → /_/`). New `looks_like_browser` helper.
- `src/events.rs` — `EventBus::clear()` drains the ring; broadcast carries a new
  `BusSignal { Event(Event), Clear }` enum so live subscribers are told to empty.
- `src/api/events.rs` — stream matches `BusSignal`, emits an SSE `event: clear`
  (ndjson `{"clear":true}`) frame; backlog still yields `Event`s.
- `src/api/mod.rs` — route `POST /_/api/events/clear` → new handler.

**Client (zero / TS + SCSS)**
- `web/src/stores/browse.ts` — mutators navigate; add `syncFromUrl` + URL codec.
- `web/src/routes/browser.ts` — `load` wires the URL sync; nav uses `navigate`.
- `web/src/app.ts` — keep `/_/browser` route; ensure query drives hydration.
- `web/src/lib/browse.ts` — `locationToUrl` / `urlToLocation` codec (+ tests).
- `web/src/routes/live-log.ts` — time-ago TIME cell (ticking `now`), Clear
  button, jump-to-object link in `Detail`.
- `web/src/lib/log.ts` — replace `elapsedLabel` with `timeAgo` (+ tests); drop
  the dead `origin` plumbing.
- `web/src/lib/api.ts` — `clearEvents()` (`POST /_/api/events/clear`).
- `web/src/lib/preview.ts` — add `"xml"` kind; `prettyJson` / `prettyXml`
  pure helpers (+ tests).
- `web/src/components/object-detail.ts` — apply pretty-print; text-preview fill.
- `web/styles/app.scss` — `.preview-pane` stops clipping tall text; theme button.
- `web/src/stores/chrome.ts` — `themePref` (`dark|light|system`), persistence,
  effective-theme resolution, live `matchMedia` listener, `cycleTheme`.
- `web/src/components/chrome.ts` — three-state cycle button.
- `web/index.html` — inline `<head>` bootstrap script: stamp `data-theme` from
  `localStorage`/`prefers-color-scheme` before first paint (no flash).
- `Cargo.toml` — bump `version` (`0.1.0` → `0.1.1`); it flows to the health
  payload (`env!("CARGO_PKG_VERSION")`, `src/api/health.rs:60`) and the top-bar
  `v…` badge automatically.
- `README.md` — documentation step.

## Risks & unknowns

- **URL sync loops / double-fetch.** Mutators `navigate()` and the effect
  fetches; the effect must diff desired-vs-current store state and no-op when
  unchanged, or navigation will re-fetch or loop. Mitigate by making the effect
  the *only* place loads happen.
- **Effect scope for `route()`.** Reading `route().query` inside a render binding
  makes it a dependency of that subtree and rebuilds it (killing input focus —
  the exact hazard `SearchToolbar` documents). The sync effect must live at a
  stable scope (route `load` / a top-level `effect`), not in a component binding.
- **Broadcast enum churn.** Changing the channel payload to `BusSignal` touches
  `subscribe`'s receiver type and every match on it; the `Lagged`/`Closed` arms
  and the existing tests must be updated together.
- **Re-rendering the TIME column on tick.** A shared `now` signal on a ~1s
  interval must update *only* the time cells (a reactive `${() => …}` binding),
  never rebuild the 2000-row table. Interval cleared on unmount.
- **XML pretty-print without a dependency** (Node-free, principle #5): a small
  regex/DOM-based indenter, not a library. Malformed input must fall back to raw.
- **`zero build` freshness.** Observing any UI box requires regenerating
  `web/dist/` and rebuilding the binary; a stale `dist/` will look like a failure.

## Steps

Each box ≈ one small commit moving an observable behavior. Check only when the
outcome is real, not when code is written.

- [x] **`.well-known` filter** — `Router::call` short-circuits any
      `/.well-known/…` request with a `404` before the S3/log path; no event is
      emitted and no stdout line prints.
- [x] **Root redirect** — a browser-shaped `GET /` (`Accept: text/html`, no
      SigV4 header/query) returns `302 Location: /_/`; every other request at `/`
      still reaches the S3 handler. Covered by a `looks_like_browser` unit test.
- [x] **Event bus clear (server)** — `EventBus::clear()` empties the ring and
      broadcasts a `Clear` signal; `POST /_/api/events/clear` calls it and
      returns `2xx`. Ring is empty on the next `subscribe(None)`; unit-tested.
- [x] **Clear reaches live subscribers** — `GET /_/api/events` emits an SSE
      `clear` frame (ndjson `{"clear":true}`) when the bus is cleared; a
      connected `curl -N` stream shows the frame after a `POST …/clear`.
- [x] **URL codec** — `locationToUrl({bucket, prefix, object})` ↔
      `urlToLocation(query)` in `lib/browse.ts`, round-tripping keys with `/` and
      spaces. Pure, unit-tested.
- [x] **In-app nav writes the URL** — selecting a bucket, drilling a folder,
      opening/closing an object update the address bar via `navigate`; browser
      Back/Forward moves between those locations (store hydrates from `route()`).
- [x] **Cold-load hydration** — loading a deep-link URL (`?bucket=&prefix=` or
      `?bucket=&object=`) enters directly into that bucket/prefix/object, not the
      default landing; no query → first bucket at root (URL normalized).
- [x] **Time-ago TIME column** — the TIME cell renders `now`/`5s`/`2m`/`1h`/`3d`
      from `ts` vs a shared `now` signal that ticks (~1s); labels advance without
      reload. `timeAgo` unit-tested; dead `origin`/`elapsedLabel` removed.
- [x] **Jump to object** — an expanded row with a `bucket` + `key` shows a "View
      object" link that `navigate`s to that object's browser URL (rides on the
      codec); rows without a key show none.
- [x] **Clear button (UI)** — a toolbar "Clear" calls `clearEvents()` and empties
      the local list; count reads `0 / 0`; new traffic appears fresh. A second
      tab empties live via the `clear` frame.
- [x] **Preview scrolls, not truncates** — a text/JSON preview taller than the
      pane scrolls first line to last with nothing clipped (`.preview-pane` no
      longer center-clips tall text; images stay centered).
- [x] **JSON/XML pretty-print** — `previewKind` gains an `"xml"` kind; JSON is
      re-indented when it parses, XML indented per element, malformed content
      falls back to raw text. `prettyJson`/`prettyXml`/`previewKind` unit-tested.
- [x] **Theme model** — `themePref` (`dark|light|system`) persists to
      `localStorage`; effective theme resolves `system` via `prefers-color-scheme`
      and updates live on OS change; inline `index.html` bootstrap stamps
      `data-theme` before first paint (no flash). Default (empty storage) =
      `system`.
- [x] **Theme cycle button** — the top-bar control cycles dark → light → system →
      dark, its glyph/label reflecting the current preference.
- [x] **Docs** — update `README.md` for the browser root-redirect front door and
      deep-linkable browser URLs; confirm nothing else user-facing changed.
- [x] **Version bump** — `Cargo.toml` `version` → `0.1.1`; the running server's
      health payload and the UI top-bar badge report `v0.1.1`.

## Progress notes

- **Routing (D):** implemented the plan's sanctioned *diff-no-op* mitigation
  rather than pure `navigate()`-only mutators. Store mutators still load and set
  signals, then push the URL via a guarded `navigate()`; the single `route()`
  effect (in `app.ts`, after `run()`) defers to `applyLocation` in a microtask
  so it tracks only `route()` (no feedback loop) and re-fetches only the diff —
  a mutator-driven nav therefore leaves the effect a no-op (no double fetch),
  while cold load / deep link / Back-Forward hydrate through it. The
  hydration/diff logic (`applyLocation`) is unit-tested at the store level;
  `navigate()` itself is guarded (no-op without a live router) so component unit
  tests still run, and its URL-writing/Back-Forward behavior is verified at
  acceptance. `loadBuckets` no longer auto-selects (the URL drives selection).
- **Clear frame (B):** the SSE clear is a *default* (unnamed) `data: {"clear":true}`
  frame rather than a named `event: clear`. A named SSE event needs
  `EventSource.addEventListener`, which trips lint rule T01 (bypasses scope
  cleanup); the unnamed frame rides the same `onmessage` channel as events (one
  handler, lint-clean) and still shows in `curl -N`. ndjson is unchanged
  (`{"clear":true}`).

- **Preview scroll (C) — real root cause.** The preview "truncate instead of
  scroll" was a *shell* height-propagation bug, not a preview-pane bug. `.app-body`
  uses the `flank` primitive, which sets `flex-wrap: wrap`; a wrapping flex
  container stretches its children to the **flex line's content height**, not the
  container height, so `.app-main` grew to its content (a tall preview → tens of
  thousands of px) instead of being pinned to the viewport. That unbounded height
  cascaded through every screen, defeating all inner `overflow:auto` scroll
  regions (the preview, and latently the live log). Fix: `.app-body { flex-wrap:
  nowrap }` so the single flex line takes the container's bounded height and
  `flex:1` descendants become height-constrained. Additionally, the object-detail
  screen and preview pane now carry the **`stack` primitive** (real
  `display:flex;flex-direction:column`) rather than the `flex-col` utility, which
  only sets `flex-direction` and never established a flex context — so
  `.detail-body`/`.preview-pane` get a bounded height to scroll within. Diagnosed
  from a live DevTools ancestor-height walk. (This fix also repairs the live log's
  scroll region, which had the same latent cause.)

- **Duplicate object-detail screen (D, found while browser-driving).** Opening an
  object via an in-app click rendered the object-detail screen *twice*. `Browser()`
  returned a bare dynamic binding (`${() => selectedObject.val ? ObjectDetail() :
  BrowseView()}`) with no stable root element. zero's router rebuilds the route
  component on *every* `/_/browser` navigation (its `_navigateTo` forces
  `divergeAt` to rebuild the leaf even on a query-only change) and swaps the result
  into the layout outlet; meanwhile the store's `selectedObject` flip re-renders
  the branch in the *currently-mounted* tree. Those two DOM operations raced over
  an overlapping node range and the outlet swap couldn't cleanly remove a section
  the inner branch-flip had just replaced — orphaning a duplicate. Fix: wrap the
  branch in a stable `<div class="browser-root stack gap-0">` the outlet owns, so
  the swap removes the whole subtree at once. `.browser-root { flex: 1; min-height:
  0 }` preserves the viewport-bounded height the inner scroll regions need. Only
  reproducible against the running app (router + store together), so verified by
  driving the browser (cold-load = 1, folder-drill = 1, in-app open = 1) with a
  render test guarding the stable-root structure.

- **Jump-to-object clobbered the target (B/D, found while browser-driving).** The
  live-log "View object" link navigated to `/_/browser?bucket=…&object=…` but the
  detail never opened — the URL was rewritten to `?bucket=…` and the browse view
  showed instead. Root cause: zero's effects are *synchronous* and its router sets
  `path`, `params`, and `query` as *separate* signals, so the route-sync effect —
  which reads both `path` and `query` — fired on the `path` change (`/_/` →
  `/_/browser`) while `query` was still the previous route's value (`{}`). That
  torn read sent an empty-bucket location into `applyLocation`, whose normalize
  branch `replace`d the URL to the first bucket, clobbering the object the correct
  second (settled-query) fire had just opened. (Cold load escapes it: the effect is
  registered *after* `run()`'s initial navigation, so its first run sees a
  consistent path+query.) Fix: `syncBrowseFromUrl` now reads the query *fresh from
  `route()` inside its deferred microtask* — after every synchronous set in the
  navigation has settled — instead of trusting the value captured at effect-fire
  time. A cleaner underlying fix would be for zero to batch the three router
  signal sets and notify once (no torn read for any subscriber), but that is a
  framework change; reading the settled URL keeps the fix in-app and robust.

- **Bucket-browser panes didn't scroll on a short window (C-adjacent, reported).**
  On a short viewport the buckets column and the folder/objects listing overflowed
  the screen instead of scrolling. Two shell-height causes, both the `flex-col` /
  `flank` gotcha again: (1) `.browser-screen` is a `flank` (`flex-wrap: wrap`), so
  its single flex line grew to the panes' *content* height rather than the
  viewport-bounded container height — the panes stretched past the screen and
  their `overflow-y: auto` never engaged. Fixed with `.browser-screen { flex-wrap:
  nowrap }`, the same fix as `.app-body`. (2) `.listing-pane` used the `flex-col`
  *utility* (only `flex-direction: column`, no `display: flex`), so it was a block
  box and `.folder-view`'s `flex: 1` couldn't claim a bounded height to scroll
  within. Switched it to the `stack` primitive (`stack gap-0`). Verified by driving
  a 1000×300 window: `.buckets-col` and `.folder-view` both bound to the screen and
  scroll (scrollTop reaches their max), and a normal 1280×720 window is unchanged.

- **Polish pass (post-acceptance, browser-driven).** Small UI refinements found
  while reviewing in a real browser, each verified by driving it:
  - *Search scope toggle.* The lone "all buckets" button floated far from the
    search box (the toolbar's `split`) with an unclear scope. Replaced with a
    `This bucket / All buckets` segmented toggle (reusing `.segmented`/`.seg-btn`)
    grouped flush beside the search input, which now fills its field (no dead gap).
  - *New-bucket affordance.* Moved the `+` from the bottom of the bucket list
    (which scrolls out of reach when the list is long) into the BUCKETS header;
    the inline name field now opens pinned under the header and dismisses on
    Escape. Scroll moved from `.buckets-col` to an inner `.buckets-list` so the
    header + form stay fixed. The `+` is now a borderless icon button (matching
    the toolbar controls), turning primary when the form is open.
  - *Long-value truncation.* DATA-DIR is middle-truncated to ~50 chars
    (`middleTruncate`) and ETAG end-truncated to 10 (`truncateEnd`), each with a
    `title` tooltip carrying the full value. `truncateEnd` is unit-tested.
  - *Chrome de-cluttering.* Dropped the top-bar ENDPOINT (redundant with the
    address bar) and the nav-footer `region us-east-1` (not useful).
  - *Live-log toolbar + scroll + order.* The filter input didn't fill its field
    (`.toolbar-filter .input { width: 100% }`, field trimmed to 18rem so the
    toolbar stays one row). The screen used the `flex-col` utility (no
    `display: flex`) so `.log-wrap` never got a bounded height to scroll — switched
    to `stack gap-0`. And the table now renders **newest-first** (reversed
    `visible`), with auto-scroll flipped to stick-to-*top* and a scroll-anchor
    (`anchorScroll`) that holds the reader's place when rows insert above. The
    Clear / Pause buttons are now icon-only with `title`/`aria-label`, so they
    take less of the toolbar row — and borderless / background-less (a muted glyph
    that brightens on hover; paused reads primary). The glyphs are now a matched
    inline-SVG set (`components/icons.ts`, solid 24×24, `currentColor`, adapted
    from Material Symbols, no npm) rather than emoji/text — the 🗑 / ❚❚ / ▶ glyphs
    sat on different baselines and drifted; the SVGs share one box and align
    exactly (verified: identical size + vertical center). zero's template parser
    namespaces the SVG children, so they render in both the browser and the test
    shim. The set was then extended to replace *every* remaining glyph used as an
    icon: row download (↓) / delete (✕), the back-crumb chevron (‹), the folder /
    file markers (📁 📄), the empty-bucket box (🗃), the new-bucket `+`, and the
    theme toggle (☾ ☀ ◐ → moon / sun / half-disc). The bucket marker (🪣) is drawn
    as an **outline pail** (tapered body, open elliptical rim, swing handle) — the
    one icon rendered as a stroke rather than a solid fill, because a filled body
    reads as a padlock or the trash icon; the outline reads as a bucket. All paths
    are Material Symbols except the outline pail (hand-drawn). Only inline text
    arrows that sit within text (the bytes column `↑`/`↓`, the "View object →"
    link) were left as text, where they align fine.
  - *Brand mark.* Replaced the top-bar `◆` glyph with a three-face **isometric
    cube** SVG — top face plus two darker sides (facet shading via `opacity`) —
    and matched the favicon to the same geometry. The in-page mark fills with
    `currentColor` so it tracks the theme; the favicon uses the indigo brand fill.
  - *`flex-col` sweep.* Confirmed no app template still uses the `flex-col`
    utility (which sets only `flex-direction`, not `display: flex`) where a real
    flex context is needed — the listing pane, bucket card, and log screen were
    the offenders, all now on the `stack` primitive. `flex-col` remains only as a
    framework utility definition, valid on already-flex elements.
  - *Bucket card.* The card used the `flex-col` utility (no `display: flex`), so
    the name and `N objects · size` were rendering inline and the size wrapped.
    Rebuilt as a `stack`: a `flank` head line (a small 🪣 glyph beside the name,
    which ellipsizes) over a full-width `N objects · size` line — kept out of the
    icon's column so it gets the whole card width and never truncates to sit
    beside the icon. Bordered active state.

- **Favicon noise (B, follow-up).** `/favicon.ico` fell through to the S3 handler
  and polluted the log (a bucket `favicon.ico` 404) — the same class of
  browser-probe noise as `.well-known`, made visible by the new root-redirect
  front door sending browsers to the S3 origin. The `.well-known` filter was
  generalized to `is_browser_probe(path)` (`/favicon.ico` or `/.well-known/…` →
  `404`, no event); the UI tab icon remains the inline data-URI in `index.html`.

## Acceptance

Mirrors the spec. `/implement` isn't done until every box passes by driving the
named client — with `web/dist/` rebuilt (`zero build`) and the binary running.

**Status.** `zero build` regenerated `web/dist/` and the debug binary (v0.1.1)
was driven on `127.0.0.1:9000`. The **HTTP-observable** boxes were driven
end-to-end with the real `curl`/`aws` clients. The **browser-visual** boxes were
subsequently driven in a real Chromium via the Playwright MCP — clicking through
theme, preview, live-log, and routing flows — and are now checked, with two
regressions found and fixed in the process (duplicate object-detail screen;
jump-to-object clobber — see Progress notes). The one box not drivable this way is
the theme's *live* OS-flip-without-reload (A2, second half): Playwright's
`emulateMedia` updates `matchMedia().matches` but does not dispatch the `change`
event, so the `watchSystemTheme` listener can't be exercised from the harness; its
logic is covered by the `resolveTheme`/`watchSystemTheme` unit tests. Fixtures live
in `demo/` (`docs/min.json`, `docs/doc.xml`, `docs/bad.json`, `docs/tall.json`,
`my docs/2026/report v2.json`).

**A. Theme**
- [x] Set theme to **dark**, reload → still dark (from `localStorage`; no dark
      flash). *(Drove the cycle button to Dark, reloaded; `data-theme=dark` and the
      dark background persisted under a light-emulated OS — proving persistence, a
      stronger discriminator than light. The inline `index.html` bootstrap stamps
      `data-theme` before first paint.)*
- [~] Set **system** with the OS dark → UI dark; flip OS to light with the page
      open → UI switches to light, no reload. *(Resolution verified — `system`
      follows the OS; the **live** flip-without-reload is not drivable via
      Playwright `emulateMedia` (no `change` event dispatched). Logic covered by
      `watchSystemTheme` + `resolveTheme` unit tests.)* `[harness-limited]`
- [x] With **light**/**dark** chosen, changing the OS setting does **not** change
      the UI. *(Drove Dark preference; UI stayed dark while the emulated OS
      preferred light — the explicit override holds over the OS. `resolveTheme`
      honors explicit overrides, unit-tested.)*
- [x] Fresh profile (empty `localStorage`) loads in the OS `prefers-color-scheme`
      mode. *(Fresh page: `localStorage` empty, emulated OS light → `data-theme`
      resolved to light.)*

**B. Live request log**
- [x] `curl -s http://localhost:9000/.well-known/appspecific/com.chrome.devtools.json`
      → absent from `curl -sN 'http://localhost:9000/_/api/events?format=ndjson'`
      and prints no stdout line; a normal `aws s3 ls` **does** appear. *(Drove
      curl + `aws s3 mb`/`ls`; ndjson log showed CreateBucket + ListBuckets and
      no `.well-known` event; server stdout had no `.well-known` line.)*
- [x] A row for a request made seconds ago reads `now`/`5s` (not `0.00s`) and its
      label advances (e.g. to `1m`) without reload. *(Drove a fresh `aws s3api
      get-object`; the row read `14s`, then ticked `26s` → `1m` over the next
      seconds with no reload.)*
- [x] Expand a `GetObject` row → a "View object" link lands on that object's
      detail view (correct bucket + key); a ListBuckets row shows none. *(Drove the
      expand + click: landed on `?bucket=demo&object=docs%2Fdoc.xml` showing that
      object's detail. This surfaced the jump-to-object clobber regression, now
      fixed — see Progress notes. Rows without a key render no link, per render
      test.)*
- [x] Click **Clear** → table empties, count `0 / 0`; a second tab on `/_/` also
      empties; later `aws s3` traffic appears fresh in both. *(Drove `POST
      /_/api/events/clear` → 204; a connected ndjson stream showed `{"clear":true}`
      then only post-clear traffic; a fresh subscriber's backlog was empty —
      the durable server-ring drain + cross-tab freshness. The UI Clear button
      emptying the table + `0 / 0` count is proven by a render test.)*

**C. Bucket browser preview** — `[human]` (logic covered by
`prettyJson`/`prettyXml`/`formatPreview`/`previewKind` unit tests + a render test
that pretty-prints a minified JSON body; fixtures seeded)
- [x] Open a JSON object taller than the pane → scrolls first line to last, no
      clipped content. *(fixture: `docs/tall.json`; drove it — pane clientHeight
      359 vs scrollHeight 33842, `overflow-y: auto`, bounded within the viewport,
      1604 pretty-printed lines.)*
- [x] Open a minified single-line JSON object → shown pretty-printed (multi-line).
      *(fixture: `docs/min.json`; drove it — rendered as indented multi-line JSON.)*
- [x] Open an XML object → shown indented, nested elements on their own lines.
      *(fixture: `docs/doc.xml`; drove it — `<catalog>`/`<book>`/`<title>` each on
      their own indented line.)*
- [x] Open a malformed-JSON object → raw text shown (no error, no blank pane).
      *(fixture: `docs/bad.json`; drove it — raw `{"a":1, "b": 2,,,` shown, no
      error, no blank pane.)*

**D. Routing / deep links**
- [x] Open an object, copy the URL, open in a new tab → same object detail loads
      directly. *(Drove a fresh navigation to `/_/browser?bucket=demo&object=docs%2Fmin.json`
      — hydrated straight into that object's detail with its pretty-printed body,
      no default landing.)*
- [x] Navigate root → folder → object, press Back twice → view returns folder →
      root, URL changes at each step. *(Drove clicks + Back/Forward: root → `docs/`
      → `min.json` → Back to `docs/` folder → Back to bucket root → Forward to
      `docs/`; URL and breadcrumb correct at every step, one screen throughout.)*
- [x] Load a deep-link URL for a folder prefix with a space and nested path (e.g.
      under `my docs/2026/`) → hydrates into that exact prefix. *(fixture:
      `my docs/2026/report v2.json`; drove `?prefix=my%20docs%2F2026%2F` — breadcrumb
      `demo / my docs / 2026`, listing `report v2.json`.)*
- [x] `curl -si -H 'Accept: text/html' http://localhost:9000/` → `302`,
      `Location: /_/`; appears in **no** live-log event. *(Drove curl; got `302`
      `location: /_/`; no redirect event in the ndjson log.)*
- [x] `aws s3 ls` still returns the bucket list (no redirect); `curl -s
      http://localhost:9000/` with no `Accept: text/html` still returns
      ListBuckets XML. *(Drove `aws s3 ls` → `demo`; anonymous `curl -s /` fell
      through to the S3 handler returning S3 XML — `AccessDenied`/"Signature is
      required" in default signed mode — not a redirect.)*
