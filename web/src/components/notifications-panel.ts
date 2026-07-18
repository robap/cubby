// The per-bucket Notifications panel (bucket browser): a list of the bucket's
// webhook destinations with a per-row delete, plus an "Add destination" form
// (url + event checkboxes + optional prefix/suffix + format). Reads the
// notifications store; mutations go through its mutators, which POST/DELETE the
// `/_/api/…/notifications` seam and refresh the list. Config takes effect
// immediately — no restart.

import { html, signal } from "zero";
import type { Signal, TemplateResult } from "zero";
import { Input, Select } from "zero/components";
import { HttpError } from "zero/http";
import type { NotificationInfo } from "../lib/api.ts";
import { selectedBucket } from "../stores/browse.ts";
import { add, closePanel, notifications, remove } from "../stores/notifications.ts";
import { TrashIcon } from "./icons.ts";

/** The event tokens a destination may subscribe to (the `s3:`-prefixed names). */
const EVENT_OPTIONS = [
  "s3:ObjectCreated:*",
  "s3:ObjectCreated:Put",
  "s3:ObjectCreated:Copy",
  "s3:ObjectCreated:CompleteMultipartUpload",
  "s3:ObjectRemoved:*",
  "s3:ObjectRemoved:Delete",
];

/**
 * The Notifications panel for the selected bucket.
 * @returns {TemplateResult}
 */
export default function NotificationsPanel(): TemplateResult {
  return html`
    <div class="notifications-panel stack gap-0">
      <div class="notifications-head split align-center pad-md border-b">
        <span class="section-label">NOTIFICATIONS · ${() => selectedBucket.val ?? ""}</span>
        <button class="button button-secondary button-sm" @click=${closePanel}>Close</button>
      </div>
      <div class="notifications-body stack gap-lg pad-md">
        ${AddForm()}
        ${List()}
      </div>
    </div>
  `;
}

/**
 * The destinations list (or the empty state).
 * @returns {TemplateResult}
 */
function List(): TemplateResult {
  // A conditionally-rendered list uses `.map()` (like FolderTable /
  // SearchResults), not `each()` — `each` is for a stable top-level binding, and
  // re-creating it inside this `() =>` conditional on every update trips its
  // reconciliation.
  return html`
    <div class="notifications-list stack gap-sm">
      ${() =>
        notifications.val.length === 0
          ? html`<div class="notifications-empty muted pad-md">No destinations yet — add one below.</div>`
          : html`${notifications.val.map(Row)}`}
    </div>
  `;
}

/**
 * One destination row: url, events, filters, format, and a delete action.
 * @param {NotificationInfo} n
 * @returns {TemplateResult}
 */
function Row(n: NotificationInfo): TemplateResult {
  const filters = [
    n.prefix ? `prefix: ${n.prefix}` : null,
    n.suffix ? `suffix: ${n.suffix}` : null,
  ].filter(Boolean).join(" · ");
  const onDelete = () => {
    const bucket = selectedBucket.val;
    if (bucket) void remove(bucket, n.id);
  };
  return html`
    <div class="notification-row border pad-sm stack gap-xs">
      <div class="split align-center gap-sm">
        <span class="notification-url mono">${n.url}</span>
        <button class="row-action row-delete" @click=${onDelete} title="Delete" aria-label="Delete">${TrashIcon()}</button>
      </div>
      <div class="notification-meta mono muted">
        ${n.events.join(", ")}${filters ? html` · ${filters}` : ""} · ${n.format} · ${n.timeout_ms}ms
      </div>
    </div>
  `;
}

/**
 * The event-subscription checkboxes: an "All events" toggle plus one box per
 * event token. Reads/writes the shared `selected` array via the passed handlers.
 * @param {Signal<string[]>} selected
 * @param {(ev: string) => void} toggle
 * @param {() => boolean} allSelected
 * @param {() => void} toggleAll
 * @returns {TemplateResult}
 */
function EventChecks(
  selected: Signal<string[]>,
  toggle: (ev: string) => void,
  allSelected: () => boolean,
  toggleAll: () => void,
): TemplateResult {
  const eventBox = (ev: string) => html`
    <label class="event-check cluster align-center gap-xs" data-event=${ev}>
      <input type="checkbox" checked=${() => selected.val.includes(ev)} @click=${() => toggle(ev)} />
      <span class="mono">${ev}</span>
    </label>
  `;
  return html`
    <div class="event-checks cluster gap-sm">
      <label class="event-check event-check-all cluster align-center gap-xs" data-event="__all__">
        <input type="checkbox" checked=${() => allSelected()} @click=${toggleAll} />
        <span class="mono">All events</span>
      </label>
      ${EVENT_OPTIONS.map(eventBox)}
    </div>
  `;
}

/**
 * The add-destination form: url, event checkboxes, optional prefix/suffix, and a
 * format select. Submitting posts through the store and clears on success; a
 * seam validation error (400) surfaces below the form.
 * @returns {TemplateResult}
 */
function AddForm(): TemplateResult {
  const url = signal("");
  const selected = signal<string[]>([]);
  const prefix = signal("");
  const suffix = signal("");
  const format = signal("s3-notification");
  const error = signal<string | null>(null);

  const toggle = (ev: string) => {
    const has = selected.val.includes(ev);
    selected.set(has ? selected.val.filter((e) => e !== ev) : [...selected.val, ev]);
  };
  // "All events" is checked only when every event is selected; clicking it
  // selects all, or clears all when already fully selected.
  const allSelected = () => selected.val.length === EVENT_OPTIONS.length;
  const toggleAll = () => selected.set(allSelected() ? [] : [...EVENT_OPTIONS]);

  // Reset every field to its initial state — called after a successful add so
  // the form is ready for the next destination.
  const reset = () => {
    url.set("");
    selected.set([]);
    prefix.set("");
    suffix.set("");
    format.set("s3-notification");
  };

  const submit = async () => {
    error.set(null);
    const bucket = selectedBucket.val;
    if (!bucket) return;
    try {
      await add(bucket, {
        url: url.val.trim(),
        events: selected.val,
        prefix: prefix.val.trim() || undefined,
        suffix: suffix.val.trim() || undefined,
        format: format.val,
      });
      reset();
    } catch (e) {
      error.set(errorMessage(e));
    }
  };

  return html`
    <form
      class="notification-add-form border pad-md stack gap-sm"
      @submit=${(e: Event) => { e.preventDefault(); void submit(); }}
    >
      <div class="text-h4">Add destination</div>
      <div class="notification-url-input">
        ${Input({ value: url, placeholder: "http://localhost:3000/hook", size: "sm" })}
      </div>
      ${EventChecks(selected, toggle, allSelected, toggleAll)}
      <div class="cluster gap-sm">
        <div class="notification-prefix-input">${Input({ value: prefix, placeholder: "prefix (optional)", size: "sm" })}</div>
        <div class="notification-suffix-input">${Input({ value: suffix, placeholder: "suffix (optional)", size: "sm" })}</div>
        <div class="notification-format-select">
          ${Select({
            value: format,
            size: "sm",
            options: [
              { value: "s3-notification", label: "s3-notification" },
              { value: "eventbridge", label: "eventbridge" },
            ],
          })}
        </div>
      </div>
      <div class="cluster align-center gap-sm">
        <button class="button button-primary button-sm" type="submit">Add</button>
        ${() => (error.val ? html`<span class="notification-error mono" role="alert">${error}</span>` : "")}
      </div>
    </form>
  `;
}

/**
 * The human-readable message from a failed request: the seam's error-envelope
 * message when present, else a generic fallback.
 * @param {unknown} e
 * @returns {string}
 */
function errorMessage(e: unknown): string {
  if (e instanceof HttpError) {
    const body = e.body as { error?: { message?: string } } | null;
    return body?.error?.message ?? `Request failed (${e.status})`;
  }
  return "Could not add destination.";
}
