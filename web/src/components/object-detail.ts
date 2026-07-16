// The object-detail sub-view of the bucket browser: a preview pane (image /
// text / JSON / download), an OBJECT metadata table, a USER METADATA table, and
// a "Generate presigned URL" card. Reached by opening an object in the browser;
// the back breadcrumb returns to the containing prefix.

import { effect, html, signal } from "zero";
import type { ObjectMeta } from "../lib/api.ts";
import type { TemplateResult } from "zero";
import { Button, Select } from "zero/components";
import { ChevronLeftIcon } from "./icons.ts";
import { contentUrl } from "../lib/api.ts";
import { fmtDate, groupDigits, humanBytes } from "../lib/format.ts";
import { EXPIRY_OPTIONS, formatPreview, previewKind } from "../lib/preview.ts";
import {
  closeObject,
  generatePresign,
  objectMeta,
  prefix,
  presignedUrl,
  selectedBucket,
  selectedObject,
} from "../stores/browse.ts";

/**
 * The object-detail screen. Reads the opened object + metadata from the browse
 * store; renders loading until metadata arrives.
 * @returns {TemplateResult}
 */
export default function ObjectDetail(): TemplateResult {
  const previewText = signal<string | null>(null);
  loadPreviewText(previewText);
  const backLabel = () => `${selectedBucket.val ?? ""}/${prefix.val}`;
  return html`
    <section class="screen detail-screen stack gap-0">
      <header class="detail-topbar split align-center pad-md border-b">
        <button class="crumb-back cluster align-center gap-xs" @click=${closeObject}>
          ${ChevronLeftIcon()}
          <span class="mono">${backLabel}</span>
        </button>
        <div class="cluster align-center gap-sm preview-label">
          <span class="chrome-label">PREVIEW</span>
          <span class="mono">${() => objectMeta.val?.content_type ?? "—"}</span>
        </div>
      </header>
      <div class="detail-body">
        <div class="preview-pane stack gap-0">${() => PreviewPane(objectMeta.val, previewText.val)}</div>
        <aside class="meta-pane stack gap-lg pad-lg">
          ${() => (objectMeta.val ? MetaTables(objectMeta.val) : html`<div class="muted">Loading…</div>`)}
          ${PresignCard()}
        </aside>
      </div>
    </section>
  `;
}

/**
 * Fetch text/JSON preview bytes into `sink` whenever the open object changes to
 * a textual type; images stream straight into an `<img>` and need no fetch.
 * @param {import("zero").Signal<string | null>} sink
 * @returns {void}
 */
function loadPreviewText(sink: { set(v: string | null): void }): void {
  effect(() => {
    const meta = objectMeta.val;
    const bucket = selectedBucket.val;
    const key = selectedObject.val;
    sink.set(null);
    if (!meta || !bucket || !key) return;
    const kind = previewKind(meta.content_type, meta.size);
    if (kind !== "text" && kind !== "json" && kind !== "xml") return;
    fetch(contentUrl(bucket, key))
      .then((r) => r.text())
      .then((t) => sink.set(t))
      .catch(() => sink.set("(failed to load preview)"));
  });
}

/**
 * The preview area: an image, monospace text/JSON, or a download affordance.
 * @param {ObjectMeta | null} meta
 * @param {string | null} textBody
 * @returns {TemplateResult}
 */
function PreviewPane(meta: ObjectMeta | null, textBody: string | null): TemplateResult {
  const bucket = selectedBucket.val;
  const key = selectedObject.val;
  if (!meta || !bucket || !key) return html`<div class="preview-empty muted">Loading…</div>`;
  const kind = previewKind(meta.content_type, meta.size);
  if (kind === "image") {
    return html`<img class="preview-img" src=${contentUrl(bucket, key)} alt=${key} />`;
  }
  if (kind === "text" || kind === "json" || kind === "xml") {
    const body = textBody === null ? "Loading…" : formatPreview(kind, textBody);
    return html`<pre class="preview-text mono">${body}</pre>`;
  }
  return html`
    <div class="preview-download stack gap-md align-center justify-center">
      <div class="muted">No inline preview for <span class="mono">${meta.content_type ?? "this type"}</span>.</div>
      <a class="button button-secondary button-md" href=${contentUrl(bucket, key)} download>Download</a>
    </div>
  `;
}

/**
 * The OBJECT + USER METADATA tables.
 * @param {ObjectMeta} meta
 * @returns {TemplateResult}
 */
function MetaTables(meta: ObjectMeta): TemplateResult {
  const userRows = Object.entries(meta.metadata ?? {});
  const row = (k: string, v: TemplateResult | string) =>
    html`<div class="meta-row"><span class="meta-k">${k}</span><span class="meta-v mono">${v}</span></div>`;
  return html`
    <div class="stack gap-md">
      <div>
        <div class="section-label">OBJECT</div>
        <div class="meta-table">
          ${row("size", `${humanBytes(meta.size)} (${groupDigits(meta.size)} bytes)`)}
          ${row("content-type", meta.content_type ?? "—")}
          ${row("etag", meta.etag)}
          ${row("last-modified", `${fmtDate(meta.last_modified)} UTC`)}
          ${row("storage-class", meta.storage_class)}
        </div>
      </div>
      ${userRows.length > 0
        ? html`
            <div>
              <div class="section-label">USER METADATA</div>
              <div class="meta-table">
                ${userRows.map(([k, v]) =>
                  html`<div class="meta-row"><span class="meta-k mono accent">x-amz-meta-${k}</span><span class="meta-v mono">${v}</span></div>`,
                )}
              </div>
            </div>
          `
        : ""}
    </div>
  `;
}

/**
 * The presigned-URL card: a GET/PUT method toggle, an expiry picker, a Generate
 * button, and the resulting URL in a copy field.
 * @returns {TemplateResult}
 */
function PresignCard(): TemplateResult {
  const method = signal<"GET" | "PUT">("GET");
  const expiry = signal(String(EXPIRY_OPTIONS[1]!.seconds)); // default 1 hour
  const options = EXPIRY_OPTIONS.map((o) => ({ value: String(o.seconds), label: o.label }));
  const methodBtn = (m: "GET" | "PUT") => html`
    <button
      class=${() => "seg-btn" + (method.val === m ? " active" : "")}
      @click=${() => method.set(m)}
    >${m}</button>
  `;
  const onGenerate = () => generatePresign(method.val, Number(expiry.val));
  return html`
    <div class="presign-card border pad-lg stack gap-md">
      <div>
        <div class="text-h4">Generate presigned URL</div>
        <div class="muted">Time-limited link, no credentials required.</div>
      </div>
      <div class="cluster gap-lg">
        <div class="stack gap-xs">
          <span class="chrome-label">METHOD</span>
          <div class="segmented cluster">${methodBtn("GET")}${methodBtn("PUT")}</div>
        </div>
        <div class="stack gap-xs presign-expiry">
          <span class="chrome-label">EXPIRES IN</span>
          ${Select({ value: expiry, options, size: "sm" })}
        </div>
      </div>
      ${Button({ variant: "primary", children: "Generate URL", onClick: onGenerate })}
      <input
        class="presign-url mono"
        readonly
        value=${presignedUrl}
        hidden=${() => !presignedUrl.val}
        @focus=${selectOnFocus}
      />
    </div>
  `;
}

/**
 * Select the field's text on focus so the URL is one keystroke from copied.
 * @param {Event} e
 * @returns {void}
 */
function selectOnFocus(e: Event): void {
  (e.target as HTMLInputElement).select();
}
