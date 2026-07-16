// The persistent app chrome: a top bar (wordmark, data-dir, endpoint, health
// dot, theme toggle) and the left nav (two INSPECT destinations + a stats
// footer). Used as the app layout, wrapping each route's outlet.

import { html, route } from "zero";
import type { TemplateResult } from "zero";
import { middleTruncate } from "../lib/format.ts";
import { MoonIcon, SunIcon, SystemIcon } from "./icons.ts";
import { cycleTheme, health, healthy, themePref } from "../stores/chrome.ts";

/** Icon + label per theme preference, so the control reflects the current one. */
const THEME_DISPLAY = {
  dark: { icon: MoonIcon, label: "Dark" },
  light: { icon: SunIcon, label: "Light" },
  system: { icon: SystemIcon, label: "System" },
} as const;

/**
 * The top bar. Reads the health store for the data-dir / endpoint / status dot.
 * @returns {TemplateResult}
 */
function TopBar(): TemplateResult {
  return html`
    <header class="topbar split align-center pad-md border-b">
      <div class="cluster align-center gap-lg">
        <div class="cluster align-center gap-sm">
          <span class="brand-mark" aria-hidden="true">
            <svg viewBox="0 0 16 16">
              <path fill="currentColor" d="M8 1 14 4.5 8 8 2 4.5Z"></path>
              <path fill="currentColor" opacity="0.78" d="M2 4.5 8 8 8 15 2 11.5Z"></path>
              <path fill="currentColor" opacity="0.55" d="M14 4.5 14 11.5 8 15 8 8Z"></path>
            </svg>
          </span>
          <span class="brand-name text-h4">cubby</span>
          <span class="badge-version mono">${() => "v" + (health.val?.version ?? "…")}</span>
        </div>
        <div class="cluster align-center gap-xs">
          <span class="chrome-label">DATA-DIR</span>
          <span class="chrome-value mono" title=${() => health.val?.data_dir ?? ""}
            >${() => middleTruncate(health.val?.data_dir ?? "…", 50)}</span>
        </div>
      </div>
      <div class="cluster align-center gap-md">
        <span class="cluster align-center gap-xs">
          <span class=${() => "status-dot " + (healthy.val ? "ok" : "down")}></span>
          <span class="status-text">${() => (healthy.val ? "healthy" : "offline")}</span>
        </span>
        <button
          class="theme-toggle cluster align-center justify-center"
          @click=${cycleTheme}
          aria-label=${() => `Theme: ${THEME_DISPLAY[themePref.val].label} (click to change)`}
          title=${() => `Theme: ${THEME_DISPLAY[themePref.val].label}`}
        >
          ${() => THEME_DISPLAY[themePref.val].icon()}
        </button>
      </div>
    </header>
  `;
}

/**
 * The left navigation: two destinations under INSPECT, plus a stats footer.
 * @returns {TemplateResult}
 */
function Nav(): TemplateResult {
  const r = route();
  const base = "nav-item split align-center";
  const logCls = () => base + (r.path === "/_" || r.path === "/_/" ? " active" : "");
  const browserCls = () => base + (r.path.startsWith("/_/browser") ? " active" : "");
  return html`
    <nav class="nav stack justify-between border-r pad-md">
      <div class="stack gap-xs">
        <div class="nav-heading">INSPECT</div>
        <a class=${logCls} href="/_/">
          <span>Live request log</span>
          <span class="live-dot" aria-hidden="true"></span>
        </a>
        <a class=${browserCls} href="/_/browser">Bucket browser</a>
      </div>
      <div class="nav-footer stack gap-xs">
        <div class="mono">
          <b>${() => health.val?.bucket_count ?? 0}</b> buckets ·
          <b>${() => health.val?.object_count ?? 0}</b> objects
        </div>
      </div>
    </nav>
  `;
}

/**
 * The app layout: chrome around the routed outlet.
 * @param {{ outlet: unknown }} props
 * @returns {TemplateResult}
 */
export default function Shell(props: { outlet: unknown }): TemplateResult {
  return html`
    <div class="app-shell">
      ${TopBar()}
      <div class="app-body flank gap-0">
        ${Nav()}
        <main class="app-main stack gap-0">${props.outlet as TemplateResult}</main>
      </div>
    </div>
  `;
}
