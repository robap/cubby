// The bucket browser (INSPECT → Bucket browser). A middle buckets column, then
// a listing pane that folder-browses a bucket (breadcrumb + folders/objects,
// per-row download & delete, drag-drop upload) or, once the search box has
// text, shows a flat substring match list with an "all buckets" scope toggle.
// Opening an object swaps in the object-detail sub-view.

import { each, effect, html, signal } from "zero";
import type { TemplateResult } from "zero";
import { Input } from "zero/components";
import { HttpError } from "zero/http";
import type { BucketInfo, ObjectInfo, SearchHit } from "../lib/api.ts";
import { contentUrl } from "../lib/api.ts";
import { crumbs, folderLabel, highlightParts, viewMode } from "../lib/browse.ts";
import { baseName, fmtDate, humanBytes, truncateEnd } from "../lib/format.ts";
import ObjectDetail from "../components/object-detail.ts";
import NotificationsPanel from "../components/notifications-panel.ts";
import CorsPanel from "../components/cors-panel.ts";
import { ArchiveIcon, BucketIcon, DownloadIcon, FileIcon, FolderIcon, PlusIcon, TrashIcon } from "../components/icons.ts";
import { closePanel, openPanel, panelOpen } from "../stores/notifications.ts";
import { closePanel as closeCors, openPanel as openCors, panelOpen as corsPanelOpen } from "../stores/cors.ts";
import {
  allBuckets,
  buckets,
  createBucket,
  folder,
  loadBuckets,
  navigateTo,
  openObject,
  prefix,
  removeObject,
  searchResults,
  searchTerm,
  selectBucket,
  selectedBucket,
  selectedObject,
  setSearch,
  toggleAllBuckets,
  uploadFiles,
} from "../stores/browse.ts";

/** Hydrate the bucket list when the route is entered. */
export function load(): Promise<void> {
  return loadBuckets();
}

/**
 * The bucket browser route. The BrowseView↔ObjectDetail choice sits inside a
 * *stable* root element rather than at the component's root: zero's router
 * rebuilds this component on every `/_/browser` navigation (including query-only
 * changes) and swaps the result into the layout outlet, while an in-app open
 * (`openObject`) also flips `selectedObject` in the currently-mounted tree. With
 * a bare root binding those two DOM operations desync and orphan a screen (the
 * detail view rendered twice); a stable wrapper the outlet owns is removed whole
 * on the swap, so exactly one screen survives.
 * @returns {TemplateResult}
 */
export default function Browser(): TemplateResult {
  return html`<div class="browser-root stack gap-0">${() => (selectedObject.val ? ObjectDetail() : BrowseView())}</div>`;
}

/**
 * The folder/search browse layout: buckets column + listing pane.
 * @returns {TemplateResult}
 */
function BrowseView(): TemplateResult {
  return html`
    <section class="screen browser-screen flank gap-0">
      ${BucketsColumn()}
      ${ListingPane()}
    </section>
  `;
}

/**
 * The middle column: a fixed BUCKETS header with a `+` new-bucket toggle, then a
 * scrolling list of buckets (each with its object count and size). Keeping `+`
 * in the header — rather than after the list — leaves it reachable when the list
 * is long. The inline name field opens just under the header (also always
 * visible) and is dismissable with Escape.
 * @returns {TemplateResult}
 */
function BucketsColumn(): TemplateResult {
  const open = signal(false);
  const name = signal("");
  const error = signal<string | null>(null);
  const close = () => {
    open.set(false);
    name.set("");
    error.set(null);
  };
  const submit = async () => {
    const trimmed = name.val.trim();
    if (!trimmed) return;
    try {
      await createBucket(trimmed);
      close();
    } catch (e) {
      error.set(errorMessage(e));
    }
  };
  const row = (b: BucketInfo) => {
    const cls = () => "bucket-row stack gap-0 text-start" + (selectedBucket.val === b.name ? " active" : "");
    const size = b.object_count > 0 ? humanBytes(b.size) : "—";
    return html`
      <button class=${cls} @click=${() => selectBucket(b.name)}>
        <span class="bucket-head flank align-center gap-sm">
          <span class="bucket-icon" aria-hidden="true">${BucketIcon()}</span>
          <span class="bucket-name mono">${b.name}</span>
        </span>
        <span class="bucket-sub mono muted">${b.object_count} objects · ${size}</span>
      </button>
    `;
  };
  return html`
    <div class="buckets-col stack gap-0">
      <div class="buckets-head split align-center pad-sm border-b">
        <span class="section-label">BUCKETS</span>
        <button
          class=${() => "new-bucket-add cluster align-center justify-center" + (open.val ? " active" : "")}
          @click=${() => (open.val ? close() : open.set(true))}
          title="New bucket"
          aria-label="New bucket"
        >${PlusIcon()}</button>
      </div>
      ${() =>
        open.val
          ? html`
              <form
                class="new-bucket-form cluster align-center gap-sm pad-sm border-b"
                @submit=${(e: Event) => { e.preventDefault(); submit(); }}
                @keydown=${(e: KeyboardEvent) => { if (e.key === "Escape") close(); }}
              >
                ${Input({ value: name, placeholder: "bucket-name", size: "sm", autofocus: true, error })}
                <button class="button button-primary button-sm" type="button" @click=${submit}>Create</button>
              </form>
            `
          : ""}
      <div class="buckets-list stack gap-xs pad-sm">
        ${each(buckets, row, (b) => b.name)}
      </div>
    </div>
  `;
}

/**
 * The human-readable message from a failed request: the seam's error envelope
 * message when present, else a generic fallback.
 * @param {unknown} e
 * @returns {string}
 */
function errorMessage(e: unknown): string {
  if (e instanceof HttpError) {
    const body = e.body as { error?: { message?: string } } | null;
    return body?.error?.message ?? `Request failed (${e.status})`;
  }
  return "Could not create bucket.";
}

/**
 * The listing pane: the search toolbar over either the folder view or the flat
 * search results, wrapped in a drag-drop upload zone.
 * @returns {TemplateResult}
 */
function ListingPane(): TemplateResult {
  const dragging = signal(false);
  const onDrop = (e: DragEvent) => {
    e.preventDefault();
    dragging.set(false);
    const files = e.dataTransfer?.files;
    if (files && files.length > 0) uploadFiles(Array.from(files));
  };
  const onDragOver = (e: DragEvent) => {
    e.preventDefault();
    dragging.set(true);
  };
  const onDragLeave = () => dragging.set(false);
  return html`
    <div
      class=${() => "listing-pane stack gap-0" + (dragging.val ? " dragging" : "")}
      @drop=${onDrop}
      @dragover=${onDragOver}
      @dragleave=${onDragLeave}
    >
      ${SearchToolbar()}
      ${() =>
        corsPanelOpen.val
          ? CorsPanel()
          : panelOpen.val
            ? NotificationsPanel()
            : viewMode(searchTerm.val) === "search"
              ? SearchResults()
              : FolderView()}
      <div class="drop-overlay align-center justify-center"><span>Drop to upload to ${() => `${selectedBucket.val ?? ""}/${prefix.val}`}</span></div>
    </div>
  `;
}

/**
 * The search box paired with a "This bucket / All buckets" scope toggle. The two
 * sit in one `.search-group` so they read as a single control: the segmented
 * toggle names what the search covers, replacing the old lone "all buckets"
 * button whose scope was unclear.
 * @returns {TemplateResult}
 */
function SearchToolbar(): TemplateResult {
  // Mirror the store's term into the field, but via an effect — NOT an eager
  // `signal(searchTerm.val)`. This component is constructed inside Browser's
  // root `${() => selectedObject.val ? … : BrowseView()}` binding, so an eager
  // read here would make `searchTerm` a dependency of that binding and rebuild
  // the whole subtree (killing the input's focus) on every keystroke. An
  // effect's reads are tracked in its own scope, so nothing leaks upward.
  const box = signal("");
  effect(() => box.set(searchTerm.val));
  const onInput = (v: string) => setSearch(v);
  // Two-state segment: clicking the inactive side flips scope (and re-runs any
  // active search); clicking the active side is a no-op.
  const scopeBtn = (label: string, all: boolean) => html`
    <button
      class=${() => "seg-btn" + (allBuckets.val === all ? " active" : "")}
      @click=${() => { if (allBuckets.val !== all) toggleAllBuckets(); }}
    >${label}</button>
  `;
  return html`
    <div class="listing-toolbar split align-center pad-md border-b">
      <div class="search-group cluster align-center gap-sm">
        <div class="search-field">
          ${Input({ value: box, placeholder: "Search keys…", size: "sm", onChange: onInput, debounceMs: 150 })}
        </div>
        <div class="segmented cluster" title="Search scope">
          ${scopeBtn("This bucket", false)}${scopeBtn("All buckets", true)}
        </div>
      </div>
      <div class="cluster align-center gap-sm">
        <span class="mono muted">
          ${() => {
            const res = searchResults.val;
            return res ? `${res.results.length} matches` : "";
          }}
        </span>
        <button
          class=${() => "cors-toggle button button-secondary button-sm" + (corsPanelOpen.val ? " active" : "")}
          @click=${toggleCors}
        >CORS</button>
        <button
          class=${() => "notifications-toggle button button-secondary button-sm" + (panelOpen.val ? " active" : "")}
          @click=${toggleNotifications}
        >Notifications</button>
      </div>
    </div>
  `;
}

/**
 * Toggle the per-bucket Notifications panel: open it (loading the bucket's
 * destinations) when closed, close it when open. A no-op with no bucket selected.
 * @returns {void}
 */
function toggleNotifications(): void {
  if (panelOpen.val) {
    closePanel();
    return;
  }
  const bucket = selectedBucket.val;
  if (bucket) {
    closeCors(); // the two panels share the listing pane — only one at a time
    void openPanel(bucket);
  }
}

/**
 * Toggle the read-only per-bucket CORS panel: open it (loading the bucket's
 * rules) when closed, close it when open. A no-op with no bucket selected.
 * @returns {void}
 */
function toggleCors(): void {
  if (corsPanelOpen.val) {
    closeCors();
    return;
  }
  const bucket = selectedBucket.val;
  if (bucket) {
    closePanel(); // the two panels share the listing pane — only one at a time
    void openCors(bucket);
  }
}

/**
 * Folder view: breadcrumb + a NAME/SIZE/MODIFIED/ETAG table of folders then
 * objects, or the empty state.
 * @returns {TemplateResult}
 */
function FolderView(): TemplateResult {
  return html`
    <div class="folder-view">
      ${Breadcrumb()}
      ${() => {
        const f = folder.val;
        if (!f) return html`<div class="pad-lg muted">Loading…</div>`;
        if (f.common_prefixes.length === 0 && f.objects.length === 0) return EmptyState();
        return FolderTable(f.common_prefixes, f.objects);
      }}
    </div>
  `;
}

/**
 * The breadcrumb trail (bucket root → each prefix segment), each crumb
 * navigable.
 * @returns {TemplateResult}
 */
function Breadcrumb(): TemplateResult {
  return html`
    <div class="breadcrumb cluster align-center gap-xs pad-md">
      ${() => {
        const bucket = selectedBucket.val;
        if (!bucket) return "";
        const trail = crumbs(bucket, prefix.val);
        return html`${trail.map((c, i) =>
          html`${i > 0 ? html`<span class="crumb-sep muted">/</span>` : ""}<button
            class="crumb mono"
            @click=${() => navigateTo(c.prefix)}
          >${c.label}</button>`,
        )}`;
      }}
    </div>
  `;
}

/**
 * The folders-then-objects table.
 * @param {string[]} commonPrefixes
 * @param {ObjectInfo[]} objects
 * @returns {TemplateResult}
 */
function FolderTable(commonPrefixes: string[], objects: ObjectInfo[]): TemplateResult {
  const cur = prefix.val;
  return html`
    <table class="listing-table">
      <thead>
        <tr><th class="c-name text-start">NAME</th><th class="c-size text-start">SIZE</th><th class="c-mod text-start">MODIFIED</th><th class="c-etag text-start">ETAG</th></tr>
      </thead>
      <tbody>
        ${commonPrefixes.map((p) => FolderRow(folderLabel(p, cur), p))}
        ${objects.map((o) => ObjectRow(o))}
      </tbody>
    </table>
  `;
}

/**
 * One folder (common-prefix) row — clicking drills into it.
 * @param {string} label
 * @param {string} fullPrefix
 * @returns {TemplateResult}
 */
function FolderRow(label: string, fullPrefix: string): TemplateResult {
  return html`
    <tr class="listing-row folder-row" @click=${() => navigateTo(fullPrefix)}>
      <td class="c-name"><span class="cluster align-center gap-sm"><span class="folder-icon" aria-hidden="true">${FolderIcon()}</span><span class="mono">${label}</span></span></td>
      <td class="c-size mono muted">—</td>
      <td class="c-mod mono muted">—</td>
      <td class="c-etag mono muted">—</td>
    </tr>
  `;
}

/**
 * One object row — name opens the detail sub-view; per-row download + delete.
 * @param {ObjectInfo} o
 * @returns {TemplateResult}
 */
function ObjectRow(o: ObjectInfo): TemplateResult {
  const bucket = selectedBucket.val!;
  // `open` sits on the name cell; the download/delete actions live in a sibling
  // cell, so their clicks never bubble through the name cell — no stopPropagation
  // needed. (Registering an event handler next to a bare `download` attribute
  // also trips zero's template parser.)
  const open = () => openObject(bucket, o.key);
  return html`
    <tr class="listing-row object-row">
      <td class="c-name" @click=${open}>
        <span class="cluster align-center gap-sm"><span class="file-icon" aria-hidden="true">${FileIcon()}</span><span class="mono link">${baseName(o.key)}</span></span>
      </td>
      <td class="c-size mono">${humanBytes(o.size)}</td>
      <td class="c-mod mono muted">${fmtDate(o.last_modified)}</td>
      <td class="c-etag mono muted">
        <span class="cluster align-center gap-sm">
          <span class="etag-val" title=${o.etag}>${truncateEnd(o.etag, 10)}</span>
          <a class="row-action row-download" href=${contentUrl(bucket, o.key)} download title="Download" aria-label="Download">${DownloadIcon()}</a>
          <button class="row-action row-delete" @click=${() => removeObject(o.key)} title="Delete" aria-label="Delete">${TrashIcon()}</button>
        </span>
      </td>
    </tr>
  `;
}

/**
 * The empty-bucket state with the drop-to-upload hint.
 * @returns {TemplateResult}
 */
function EmptyState(): TemplateResult {
  return html`
    <div class="empty-state text-center stack gap-sm align-center justify-center">
      <div class="empty-icon" aria-hidden="true">${ArchiveIcon()}</div>
      <div>No objects yet.</div>
      <div class="muted">Drop files to upload to <span class="mono">${() => `${selectedBucket.val ?? ""}/${prefix.val}`}</span></div>
    </div>
  `;
}

/**
 * The flat search result list: each full key with the term highlighted, sized
 * and dated, tagged with its bucket when the scope is all-buckets.
 * @returns {TemplateResult}
 */
function SearchResults(): TemplateResult {
  return html`
    <div class="search-results">
      ${() => {
        const res = searchResults.val;
        if (!res) return html`<div class="pad-lg muted">Searching…</div>`;
        if (res.results.length === 0) {
          const term = searchTerm.val;
          return html`<div class="pad-lg muted">No keys match “${term}”.</div>`;
        }
        return html`<table class="listing-table search-table"><tbody>${res.results.map((h) => SearchRow(h))}</tbody></table>`;
      }}
    </div>
  `;
}

/**
 * One flat search hit — highlighted key, size, modified; opens the object.
 * @param {SearchHit} h
 * @returns {TemplateResult}
 */
function SearchRow(h: SearchHit): TemplateResult {
  const parts = highlightParts(h.key, searchTerm.val);
  return html`
    <tr class="listing-row search-row" @click=${() => openObject(h.bucket, h.key)}>
      <td class="c-name">
        <span class="cluster align-center gap-sm">
          ${allBuckets.val ? html`<span class="bucket-tag mono">${h.bucket}</span>` : ""}
          <span class="mono">${parts.map((p) => (p.match ? html`<mark>${p.text}</mark>` : p.text))}</span>
        </span>
      </td>
      <td class="c-size mono">${humanBytes(h.size)}</td>
      <td class="c-mod mono muted">${fmtDate(h.last_modified)}</td>
    </tr>
  `;
}
