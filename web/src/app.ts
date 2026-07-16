// App entry: wires the persistent chrome (Shell) around the two INSPECT
// screens — the live request log (home) and the bucket browser — mounted under
// the `/_/` prefix the binary serves the SPA from. Health is loaded once and
// polled so the top bar / nav footer stay live as buckets and objects change.

import { App, effect, route } from "zero";
import Shell from "./components/chrome.ts";
import LiveLog from "./routes/live-log.ts";
import Browser, { load as loadBrowser } from "./routes/browser.ts";
import { applyTheme, loadHealth, watchSystemTheme } from "./stores/chrome.ts";
import { syncBrowseFromUrl } from "./stores/browse.ts";

applyTheme();
watchSystemTheme();
loadHealth();

const app = new App()
  .layout(Shell)
  .route("/_/", LiveLog)
  .route("/_", LiveLog)
  .route("/_/browser", Browser, { load: loadBrowser })
  .route("*", LiveLog);
app.run("#app");

// The URL is the source of truth for the bucket browser: hydrate on cold load
// and on every Back/Forward. Registered after `run()` (which `route()` needs),
// this effect tracks only `route()` — `syncBrowseFromUrl` defers the store work
// to a microtask so those reads never feed back into this effect.
//
// Read both `path` and `query` so the effect re-fires on either changing (e.g. a
// query-only Back/Forward). zero's effects are synchronous and its router sets
// `path` and `query` as separate signals, so this can fire mid-navigation with a
// torn (new-path, stale-query) view — `syncBrowseFromUrl` guards against that by
// re-reading the settled `route()` inside its microtask rather than trusting a
// value captured here.
effect(() => {
  const r = route();
  const path = r.path;
  void r.query;
  if (path === "/_/browser") syncBrowseFromUrl();
});
