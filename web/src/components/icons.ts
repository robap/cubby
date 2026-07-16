// Inline SVG icons: a small, matched set of solid 24×24 glyphs used across the
// chrome (toolbar controls, row actions, type markers, the theme toggle).
// Because every icon shares one viewBox and box model, controls line up exactly
// — the emoji / text glyphs they replace (🗑 ❚❚ ▶ ↓ ✕ ‹ 📁 📄 🪣 ☾ ☀ ◐) sat on
// different baselines and drifted. `currentColor` tracks the theme. Path data is
// adapted from Google's Material Symbols (Apache-2.0), except the outline bucket
// (hand-drawn; see BucketIcon), kept in-repo so there is no npm dependency and
// the bundle stays self-contained.
//
// Add an icon by exporting another function returning one complete `<svg>`
// template (a single template so zero's parser namespaces its children as SVG).

import { html } from "zero";
import type { TemplateResult } from "zero";

/** Trash can — "clear the log" and per-row delete (Material `delete`). */
export function TrashIcon(): TemplateResult {
  return html`<svg class="icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M7 21a2 2 0 0 1-2-2V6H4V4h5V3h6v1h5v2h-1v13a2 2 0 0 1-2 2H7ZM9 17h2V8H9v9Zm4 0h2V8h-2v9Z"></path></svg>`;
}

/** Two bars — pause the live tail (Material `pause`). */
export function PauseIcon(): TemplateResult {
  return html`<svg class="icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><rect x="6" y="5" width="4" height="14" rx="1"></rect><rect x="14" y="5" width="4" height="14" rx="1"></rect></svg>`;
}

/** Right-pointing triangle — resume the live tail (Material `play_arrow`). */
export function PlayIcon(): TemplateResult {
  return html`<svg class="icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M8 5v14l11-7L8 5Z"></path></svg>`;
}

/** Plus — the "new bucket" toggle (Material `add`). */
export function PlusIcon(): TemplateResult {
  return html`<svg class="icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z"></path></svg>`;
}

/** Down arrow into a tray — download an object (Material `file_download`). */
export function DownloadIcon(): TemplateResult {
  return html`<svg class="icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M19 9h-4V3H9v6H5l7 7 7-7zM5 18v2h14v-2H5z"></path></svg>`;
}

/** Left chevron — the back breadcrumb (Material `chevron_left`). */
export function ChevronLeftIcon(): TemplateResult {
  return html`<svg class="icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M15.41 7.41 14 6l-6 6 6 6 1.41-1.41L10.83 12z"></path></svg>`;
}

/** Folder — a common-prefix "folder" row (Material `folder`). */
export function FolderIcon(): TemplateResult {
  return html`<svg class="icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M10 4H4c-1.1 0-1.99.9-1.99 2L2 18c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V8c0-1.1-.9-2-2-2h-8l-2-2z"></path></svg>`;
}

/** Document — an object row (Material `insert_drive_file`). */
export function FileIcon(): TemplateResult {
  return html`<svg class="icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M6 2c-1.1 0-1.99.9-1.99 2L4 20c0 1.1.89 2 1.99 2H18c1.1 0 2-.9 2-2V8l-6-6H6zm7 7V3.5L18.5 9H13z"></path></svg>`;
}

/** A pail — the bucket card marker. Drawn as an outline (a tapered body, an
 * open elliptical rim, and a swing handle) rather than a solid fill: a filled
 * body reads as a padlock or the trash icon, an outline reads as a bucket. */
export function BucketIcon(): TemplateResult {
  return html`<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round" stroke-linecap="round" aria-hidden="true"><path d="M4.5 8l1.6 11.2a1.6 1.6 0 0 0 1.58 1.38h8.64a1.6 1.6 0 0 0 1.58-1.38L19.5 8"></path><ellipse cx="12" cy="8" rx="7.5" ry="2"></ellipse><path d="M6 7.2C6.7 3 9 1.5 12 1.5s5.3 1.5 6 5.7"></path></svg>`;
}

/** A box — the empty-bucket state marker (Material `inventory_2`). */
export function ArchiveIcon(): TemplateResult {
  return html`<svg class="icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M20 2H4c-1.1 0-2 .9-2 2v3.01c0 .72.43 1.34 1 1.69V20c0 1.1 1.1 2 2 2h14c.9 0 2-.9 2-2V8.7c.57-.35 1-.97 1-1.69V4c0-1.1-.9-2-2-2zm-5 12H9v-2h6v2zm5-7H4V4h16v3z"></path></svg>`;
}

/** Crescent — the dark theme preference (Material `dark_mode`). */
export function MoonIcon(): TemplateResult {
  return html`<svg class="icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M12 3c-4.97 0-9 4.03-9 9s4.03 9 9 9 9-4.03 9-9c0-.46-.04-.92-.1-1.36-.98 1.37-2.58 2.26-4.4 2.26-2.98 0-5.4-2.42-5.4-5.4 0-1.81.89-3.42 2.26-4.4C12.92 3.04 12.46 3 12 3z"></path></svg>`;
}

/** Sun — the light theme preference (Material `light_mode`). */
export function SunIcon(): TemplateResult {
  return html`<svg class="icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M12 7a5 5 0 1 0 0 10 5 5 0 0 0 0-10zM2 13h2a1 1 0 0 0 0-2H2a1 1 0 0 0 0 2zm18 0h2a1 1 0 0 0 0-2h-2a1 1 0 0 0 0 2zM11 2v2a1 1 0 0 0 2 0V2a1 1 0 0 0-2 0zm0 18v2a1 1 0 0 0 2 0v-2a1 1 0 0 0-2 0zM5.99 4.58a1 1 0 0 0-1.41 1.41l1.06 1.06a1 1 0 0 0 1.41-1.41L5.99 4.58zm12.37 12.37a1 1 0 0 0-1.41 1.41l1.06 1.06a1 1 0 0 0 1.41-1.41l-1.06-1.06zm1.06-10.96a1 1 0 0 0-1.41-1.41l-1.06 1.06a1 1 0 0 0 1.41 1.41l1.06-1.06zM7.05 18.36a1 1 0 0 0-1.41-1.41l-1.06 1.06a1 1 0 0 0 1.41 1.41l1.06-1.06z"></path></svg>`;
}

/** A half-filled disc — the "follow the OS" (system) theme preference. */
export function SystemIcon(): TemplateResult {
  return html`<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><circle cx="12" cy="12" r="9"></circle><path fill="currentColor" stroke="none" d="M12 3a9 9 0 0 0 0 18Z"></path></svg>`;
}
