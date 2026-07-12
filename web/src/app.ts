// App entry: wires the persistent chrome (Shell) around the two INSPECT
// screens — the live request log (home) and the bucket browser — mounted under
// the `/_/` prefix the binary serves the SPA from. Health is loaded once and
// polled so the top bar / nav footer stay live as buckets and objects change.

import { App } from "zero";
import Shell from "./components/chrome.ts";
import LiveLog from "./routes/live-log.ts";
import Browser, { load as loadBrowser } from "./routes/browser.ts";
import { applyTheme, loadHealth } from "./stores/chrome.ts";

applyTheme();
loadHealth();

new App()
  .layout(Shell)
  .route("/_/", LiveLog)
  .route("/_", LiveLog)
  .route("/_/browser", Browser, { load: loadBrowser })
  .route("*", LiveLog)
  .run("#app");
