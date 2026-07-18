// The read-only per-bucket CORS panel (bucket browser): the bucket's configured
// CORS rules — origins, methods, allowed/expose headers, max-age — or a "no CORS
// configured" empty state. Display only: management stays the real S3 API
// (PutBucketCors/DeleteBucketCors), the fidelity point, so there is no
// add/edit/delete control here. Reads the cors store, loaded from the read-only
// `/_/api/buckets/{bucket}/cors` seam; reflects the live config with no restart.

import { html } from "zero";
import type { TemplateResult } from "zero";
import type { CorsInfo } from "../lib/api.ts";
import { selectedBucket } from "../stores/browse.ts";
import { closePanel, rules } from "../stores/cors.ts";

/**
 * The CORS panel for the selected bucket.
 * @returns {TemplateResult}
 */
export default function CorsPanel(): TemplateResult {
  return html`
    <div class="cors-panel stack gap-0">
      <div class="cors-head split align-center pad-md border-b">
        <span class="section-label">CORS · ${() => selectedBucket.val ?? ""}</span>
        <button class="button button-secondary button-sm" @click=${closePanel}>Close</button>
      </div>
      <div class="cors-body stack gap-md pad-md">
        <div class="cors-note muted">
          Read-only. Configure CORS with the S3 API
          (<span class="mono">aws s3api put-bucket-cors</span>).
        </div>
        ${List()}
      </div>
    </div>
  `;
}

/**
 * The rules list (or the empty state). A conditionally-rendered list uses
 * `.map()` (like the notifications/folder views), not `each()`.
 * @returns {TemplateResult}
 */
function List(): TemplateResult {
  return html`
    <div class="cors-list stack gap-sm">
      ${() => {
        const rs = rules.val;
        if (!rs || rs.length === 0) {
          return html`<div class="cors-empty muted pad-md">No CORS configured for this bucket.</div>`;
        }
        return html`${rs.map(Rule)}`;
      }}
    </div>
  `;
}

/**
 * One CORS rule row: origins and methods, then optional allowed/expose headers
 * and max-age. Absent optional fields are omitted.
 * @param {CorsInfo} r
 * @returns {TemplateResult}
 */
function Rule(r: CorsInfo): TemplateResult {
  return html`
    <div class="cors-rule border pad-sm stack gap-xs">
      ${Field("Origins", (r.AllowedOrigins ?? []).join(", "))}
      ${Field("Methods", (r.AllowedMethods ?? []).join(", "))}
      ${r.AllowedHeaders && r.AllowedHeaders.length > 0
        ? Field("Allowed headers", r.AllowedHeaders.join(", "))
        : ""}
      ${r.ExposeHeaders && r.ExposeHeaders.length > 0
        ? Field("Expose headers", r.ExposeHeaders.join(", "))
        : ""}
      ${typeof r.MaxAgeSeconds === "number" ? Field("Max-Age", `${r.MaxAgeSeconds}s`) : ""}
    </div>
  `;
}

/**
 * One labelled field within a rule row.
 * @param {string} label
 * @param {string} value
 * @returns {TemplateResult}
 */
function Field(label: string, value: string): TemplateResult {
  return html`
    <div class="cors-field flank align-center gap-sm">
      <span class="cors-field-label muted">${label}</span>
      <span class="cors-field-value mono">${value}</span>
    </div>
  `;
}
