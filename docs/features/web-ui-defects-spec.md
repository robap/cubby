# Web UI defects — spec

**Status:** implemented — HTTP acceptance driven; browser-visual acceptance pending human verification · **Roadmap:** ad-hoc (Phase 5 follow-up) · **Slug:** web-ui-defects

## Why

The web UI shipped in Phase 5 and is the differentiator — cubby is an *S3
debugger*, not just a stand-in (CONCEPT §"Web UI"; ROADMAP Phase 5). Living with
it surfaced a batch of rough edges collected in `defects.md`. Each one dulls the
two north stars this UI exists to serve:

- **Dev tool first** (north star 4): log readability and browse ergonomics are
  the feature. A log polluted with browser noise, a time column you can't read
  at a glance, no way to clear it, and no jump-to-object all cut against that.
- **The filesystem is the API too** (north star 2): the browser is the on-ramp
  to those files; if previews truncate and you can't deep-link or bookmark an
  object, the browser is harder to trust than `ls`/`cat`.

None of this adds surface area from the non-goals list — it is polish on
behavior that already exists.

## In scope

Grouped by the four areas in `defects.md`:

**A. Theme (`stores/chrome.ts`, `components/chrome.ts`)**
- Persist the chosen theme across browser sessions.
- A third mode, **system**, that follows the OS `prefers-color-scheme` and
  tracks live changes to it.

**B. Live request log (`routes/live-log.ts`, `src/http.rs`, `src/events.rs`)**
- Stop capturing browser-probe noise (`/.well-known/…`) in the S3 request log.
- Replace the elapsed-since-first-event TIME column with a human "time-ago"
  format that stays meaningful as time passes.
- From an expanded row, a way to jump to that object in the bucket browser.
- A control to clear the log.

**C. Bucket browser preview (`components/object-detail.ts`, `lib/preview.ts`,
`web/styles/app.scss`)**
- Tall text/JSON previews scroll instead of truncating.
- JSON and XML previews are pretty-printed (indented).

**D. Routing (`app.ts`, `stores/browse.ts`, `src/http.rs`)**
- Deep links to browser locations — a bucket, a folder prefix, and an open
  object — that survive reload, are copy-pasteable, and drive browser
  back/forward. This is the substrate defect B's jump-to-object rides on.
- Bare `GET /` from a browser redirects to `/_/` (the documented front-door
  behavior, not yet built).

## Out of scope

- Any new S3 API surface or anything from the CONCEPT non-goals (versioning,
  tagging, IAM, …). This is UI-only plus one server-side log filter.
- Log **persistence** across restarts — the log resetting on restart is correct
  for a dev tool (CONCEPT §"Live log design"). "Clear" is a live-view action,
  not durable state.
- Syntax highlighting / colorized previews. "Readable text" is the bar, not an
  editor.
- Full-text or per-line search inside a preview.
- Virtual-host addressing or any change to path-style routing (v1.1).
- Persisting theme server-side — it is a per-browser preference.

## Behavior

### A. Theme

Today `theme` is a `"dark" | "light"` signal; `toggleTheme` flips it and stamps
`<html data-theme>`, and `readInitialTheme` reads an existing `data-theme` attr
or `matchMedia`, but nothing is ever persisted. Change to a three-way
**preference** — `"dark" | "light" | "system"`:

- The preference persists (localStorage). On next load the same preference is
  restored before first paint (no flash of the wrong theme).
- `"system"` resolves the *effective* theme from `prefers-color-scheme` and
  updates live when the OS setting changes while the page is open.
- `"dark"` / `"light"` are explicit overrides that ignore the OS.
- The top-bar control is a **three-state cycle button**: each click advances
  dark → light → system → dark. Its icon/label always reflects the current
  preference (a plain sun/moon two-state can't express "system" — e.g. a
  sun / moon / monitor-or-"auto" glyph).
- The default preference for a brand-new browser (nothing stored) is
  **system** — cubby should match the environment out of the box.

### B. Live request log

**`.well-known` noise.** A browser pointed at `/_/` fires background probes such
as `GET /.well-known/appspecific/com.chrome.devtools.json`. These don't start
with `/_/`, so `Router::call` hands them to `log_and_serve_s3`, which records
them as S3 events (bucket `.well-known`, a 404 `NoSuchBucket`). They are never
real S3 traffic. Short-circuit `/.well-known/…` before the log/S3 path so no
event is emitted and no pretty stdout line prints; respond `404` (its body is
irrelevant). The S3 log must contain only S3-intent requests.

**TIME column.** Today the column shows seconds since the first event
(`elapsedLabel` → `"1.23s"`), which is unreadable once the log is minutes old
and meaningless after a clear. Show time **relative to now** — e.g. `now`,
`5s`, `2m`, `1h`, `3d` — derived from the event's wall-clock `ts` (already Unix
ms). Labels re-render as time passes (a coarse tick is fine; no per-frame
churn). The absolute timestamp remains available in the expanded detail row.

**Jump to object.** The expanded detail row (`Detail`) currently lists fields
only. When an event has both a `bucket` and a `key` and resolves to an object
operation, add an affordance ("View object" / "Open in browser") that navigates
to that object's detail view in the bucket browser (rides on D). Events without
a key (ListBuckets, CreateBucket, errors before resolution) show no such link.

**Clear log.** Add a "Clear" control to the toolbar. Clicking it empties the
visible table (count → `0 / 0`) and resets the relative-time origin. Because
`EventSource` auto-reconnects and the server replays its ring buffer, a
client-only clear would repopulate on the next reconnect and would not affect
other open tabs — so clear must empty the server-side ring too (a new
`POST /_/api/events/clear` that drains the `EventBus` ring), making the clear
durable for reconnects and consistent across tabs. After a clear, only events
that arrive *after* the clear appear.

### C. Bucket browser preview

**Scroll, not truncate.** `.preview-pane` centers its child
(`flex-row align-center justify-center`); a `<pre>` taller than the pane gets
centered and its overflow above the fold becomes unreachable — the top of a long
JSON file is clipped and unscrollable. A text/JSON preview taller than the pane
must scroll to reveal every line, first line to last, with nothing clipped.

**JSON & XML pretty-printing.** `previewKind` already maps `application/json`,
`+json`, `text/*`, and `application/xml` to a preview; the defect is that the
bytes are shown **raw**, so a minified JSON blob or a single-run XML document is
unreadable. Both should be pretty-printed in the preview: JSON re-indented when
it parses as JSON, XML indented with nested elements on their own lines. Content
that fails to parse (malformed JSON, non-well-formed XML) falls back to the raw
text rather than erroring or blanking. Pretty-printing happens client-side after
the fetch, within the existing `PREVIEW_MAX_BYTES` cap.

### D. Routing / deep links

Today browser state (`selectedBucket`, `prefix`, `selectedObject`) lives only in
`stores/browse.ts` signals; the router knows just `/_/` and `/_/browser`. So a
reload drops you back to the first bucket's root, nothing is linkable, and
back/forward doesn't move within the browser.

Encode the browser location in the URL so it is linkable, reloadable, and
history-navigable. The exact scheme (path segments vs. query params, and how a
folder prefix is disambiguated from an object key) is a `/plan` decision, but the
observable contract is fixed:

- A bucket, a nested folder prefix, and an open object are each addressable by a
  distinct URL.
- Loading such a URL cold hydrates directly into that location (right bucket,
  prefix, and — if an object — its detail view), not the default landing state.
- In-app navigation (selecting a bucket, drilling into a folder, opening an
  object, the back-crumb) updates the URL; browser Back/Forward moves between
  those locations.
- Percent-encoded keys with `/` and spaces round-trip correctly.

**Root redirect.** Today `Router::call` (`src/http.rs:92`) hands everything that
isn't `/_/…` to the S3 handler, so a browser pointed at `http://localhost:9000/`
gets ListBuckets XML instead of the UI. Per CONCEPT §"Single port, routed", a
bare `GET /` that looks like a human's browser — carries `Accept: text/html`
**and** presents no SigV4 auth (no `Authorization` header, no `X-Amz-*` query
params) — redirects (`302`, `Location: /_/`). Any other request at `/` — an SDK's
signed ListBuckets, or anything not asking for HTML — falls through to the S3
handler exactly as today. Keying on the `Accept` header keeps this robust even
under accept-any-credentials mode, since a real S3 client never asks for HTML.
The redirect is a pure routing decision and emits no request-log event.

## Acceptance criteria

Each is satisfiable by watching something happen — a browser action, an HTTP
probe, or a reload. Grouped by area; this list becomes the plan's checkboxes.

**A. Theme**
- [ ] In a browser, set theme to **light**, reload the page → the UI is still
      light (preference restored from localStorage; no dark flash).
- [ ] Set theme to **system** with the OS in dark mode → UI is dark; flip the OS
      to light while the page stays open → UI switches to light with no reload.
- [ ] With **light** or **dark** explicitly chosen, changing the OS setting does
      **not** change the UI.
- [ ] A fresh browser profile (empty localStorage) loads in the mode matching
      the OS `prefers-color-scheme` (system default).

**B. Live request log**
- [ ] `curl -s http://localhost:9000/.well-known/appspecific/com.chrome.devtools.json`
      → the request does **not** appear in
      `curl -sN 'http://localhost:9000/_/api/events?format=ndjson'` and prints no
      pretty stdout line; a normal `aws s3 ls` against the same server **does**
      appear.
- [ ] In the live-log UI, a row for a request made seconds ago reads `now`/`5s`
      (not `0.00s`), and its TIME label advances (e.g. to `1m`) as time passes
      without a reload.
- [ ] Expand a `GetObject` row in the log → a "View object" affordance is
      present; clicking it lands on that object's detail view in the bucket
      browser (correct bucket + key). A ListBuckets row shows no such
      affordance.
- [ ] With events visible, click **Clear** → the table empties and the count
      shows `0 / 0`; a second browser tab open on `/_/` also empties (server
      ring drained); subsequent `aws s3` traffic appears fresh in both.

**C. Bucket browser preview**
- [ ] Open a JSON object taller than the preview pane → the preview scrolls
      first line to last with no clipped/unreachable content.
- [ ] Upload a minified single-line JSON object, open it → the preview shows it
      pretty-printed (indented, multi-line), not one long run.
- [ ] Open an XML object → the preview shows it indented with nested elements on
      their own lines.
- [ ] Open a JSON object whose bytes are malformed → the preview shows the raw
      text (no thrown error, no blank pane).

**D. Routing / deep links**
- [ ] Open an object in the browser, copy the address-bar URL, open it in a new
      tab → the same object's detail view loads directly (correct bucket + key),
      not the default landing screen.
- [ ] Navigate root → folder → object, then press browser **Back** twice → the
      view returns folder → root, and the URL changes at each step.
- [ ] Load a deep-link URL for a folder prefix with a space and a nested path
      (e.g. a key under `my docs/2026/`) → the browser hydrates into that exact
      prefix.
- [ ] `curl -si -H 'Accept: text/html' http://localhost:9000/` → `302` with
      `Location: /_/`; the same request appears in **no** live-log event.
- [ ] `aws s3 ls` (signed ListBuckets) against the same server still returns the
      bucket list — no redirect — and `curl -s http://localhost:9000/` with no
      `Accept: text/html` still returns ListBuckets XML.

## Resolved decisions

All open questions are settled — the spec above reflects these:

1. **One plan.** All four areas ship under the single `web-ui-defects` slug,
   sequenced so **D (routing)** lands before **B's jump-to-object**, which
   depends on it; the other items are independent.
2. **Clear drains the server ring.** `POST /_/api/events/clear` empties the
   `EventBus` ring, so the clear survives `EventSource` reconnect and is
   consistent across open tabs.
3. **Preview = pretty-print, not detection.** "More preview types for json, xml"
   means pretty-printing (indenting) JSON and XML that already preview, not
   content-type/extension fallback. Area C is scoped accordingly.
4. **Theme control = three-state cycle button** (dark → light → system → dark).
