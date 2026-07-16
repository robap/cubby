// Live request log — the home screen. Subscribes to the SSE stream, batches
// incoming events per animation frame, and renders a dense, colour-coded table
// with filter / pause / auto-scroll / click-to-expand.

import { computed, each, effect, html, ref, signal } from "zero";
import type { Signal, TemplateResult } from "zero";
import { Input, Select } from "zero/components";
import type { LogEvent } from "../lib/api.ts";
import { clearEvents } from "../lib/api.ts";
import { bytesCell, statusClass, targetOf } from "../lib/format.ts";
import { PauseIcon, PlayIcon, TrashIcon } from "../components/icons.ts";
import { locationToUrl, parentPrefix } from "../lib/browse.ts";
import { appendCapped, matchesFilter, timeAgo } from "../lib/log.ts";

/** Cap on retained rows (matches the server ring's spirit). */
const MAX_ROWS = 2000;
/** Duration above which a request is highlighted as slow. */
const SLOW_MS = 100;

/** Reactive state + imperative handles for one live-log screen instance. */
type LogState = {
  events: Signal<LogEvent[]>;
  visible: { val: LogEvent[] };
  paused: Signal<boolean>;
  newCount: Signal<number>;
  filter: Signal<string>;
  statusFilter: Signal<string>;
  authFilter: Signal<string>;
  expanded: Signal<number | null>;
  scroller: ReturnType<typeof ref<HTMLElement>>;
  /** A coarse "current time" that ticks ~1s so the TIME cells advance. */
  now: Signal<number>;
  resume: () => void;
  clear: () => void;
  onScroll: () => void;
};

/**
 * @returns {TemplateResult}
 */
export default function LiveLog(): TemplateResult {
  const s = createLogState();
  return html`
    <section class="screen log-screen stack gap-0">
      ${Toolbar(s)}
      <div class="log-wrap" ref=${s.scroller} @scroll=${s.onScroll}>
        ${LogTable(s)}
        ${() =>
          s.visible.val.length === 0
            ? html`<div class="empty-state text-center">Waiting for S3 traffic…</div>`
            : ""}
      </div>
    </section>
  `;
}

/**
 * Assemble the reactive state bundle: signals, the SSE ingest machinery, and
 * the filtered `visible` view.
 * @returns {LogState}
 */
function createLogState(): LogState {
  const events = signal<LogEvent[]>([]);
  const paused = signal(false);
  const newCount = signal(0);
  const filter = signal("");
  const statusFilter = signal("all");
  const authFilter = signal("any");
  const expanded = signal<number | null>(null);
  const scroller = ref<HTMLElement>();

  // A shared clock that ticks once a second, so the relative TIME labels advance
  // without touching the 2000-row table body (only the per-cell `${…}` bindings
  // that read `now` re-run). The interval is cleared when the screen unmounts.
  const now = signal(Date.now());
  effect(() => {
    const id = setInterval(() => now.set(Date.now()), 1000);
    return () => clearInterval(id);
  });

  const ingest = createIngest(events, paused, newCount, scroller);
  // Newest first: the freshest request sits at the top of the table. `filter`
  // returns a fresh array, so reversing it never mutates the stored ring.
  const visible = computed(() =>
    events.val
      .filter((e) => matchesFilter(e, filter.val, statusFilter.val, authFilter.val))
      .reverse(),
  );

  return {
    events,
    visible,
    paused,
    newCount,
    filter,
    statusFilter,
    authFilter,
    expanded,
    scroller,
    now,
    resume: ingest.resume,
    clear: ingest.clear,
    onScroll: ingest.onScroll,
  };
}

/**
 * Open the live SSE stream and hand each frame's `data` to `onFrame`, closing
 * the source when the screen unmounts (the effect's cleanup).
 * @param {(data: string) => void} onFrame
 * @returns {void}
 */
function subscribeSse(onFrame: (data: string) => void): void {
  effect(() => {
    const es = new EventSource("/_/api/events");
    es.onmessage = (msg) => onFrame(msg.data);
    return () => es.close();
  });
}

/**
 * Position the scroller after newest-first rows are inserted at the top: reveal
 * them when `toTop`, else hold the reader's place by the inserted height delta.
 * @param {HTMLElement} el
 * @param {boolean} toTop
 * @param {number} prevTop
 * @param {number} prevHeight
 * @returns {void}
 */
function anchorScroll(el: HTMLElement, toTop: boolean, prevTop: number, prevHeight: number): void {
  el.scrollTop = toTop ? 0 : prevTop + (el.scrollHeight - prevHeight);
}

/**
 * Wire the SSE stream to `events` with per-animation-frame batching, pause
 * buffering, and auto-scroll bookkeeping. Returns the imperative handles the
 * view needs (`resume`, `onScroll`).
 * @returns {{ resume: () => void, onScroll: () => void }}
 */
function createIngest(
  events: Signal<LogEvent[]>,
  paused: Signal<boolean>,
  newCount: Signal<number>,
  scroller: ReturnType<typeof ref<HTMLElement>>,
): { resume: () => void; clear: () => void; onScroll: () => void } {
  let stick = true; // pinned to the top → new rows (newest first) stay in view
  let incoming: LogEvent[] = []; // buffered until the next frame / resume
  let rafScheduled = false;

  // Empty the local view: drop the retained rows, any buffered batch, and the
  // paused "N new" count. Driven both by the toolbar Clear and by a server
  // `clear` frame (so other tabs empty when one clears).
  const clear = () => {
    incoming = [];
    events.set([]);
    newCount.set(0);
  };

  // Rows render newest-first, so a new event lands at the *top*. Pinned to the
  // top we reveal it; if the reader has scrolled down into older rows we hold
  // their position as rows insert above (see `anchorScroll`).
  const append = (batch: LogEvent[]) => {
    if (batch.length === 0) return;
    const el = scroller.el;
    const prevHeight = el?.scrollHeight ?? 0;
    const prevTop = el?.scrollTop ?? 0;
    events.update((prev) => appendCapped(prev, batch, MAX_ROWS));
    if (el) anchorScroll(el, stick && !paused.val, prevTop, prevHeight);
  };

  const flush = () => {
    rafScheduled = false;
    if (incoming.length === 0) return;
    if (paused.val) {
      newCount.set(incoming.length);
      return; // keep buffered; resume flushes them
    }
    const batch = incoming;
    incoming = [];
    append(batch);
  };

  const schedule = () => {
    if (!rafScheduled) {
      rafScheduled = true;
      requestAnimationFrame(flush);
    }
  };

  subscribeSse((data) => {
    try {
      const parsed = JSON.parse(data) as Partial<LogEvent> & { clear?: boolean };
      // The server's clear rides the default data channel (`{"clear":true}`),
      // so this one handler empties the view when any tab clears the ring.
      if (parsed.clear === true) {
        clear();
        return;
      }
      if (typeof parsed.id === "number") {
        incoming.push(parsed as LogEvent);
        schedule();
      }
    } catch {
      /* ignore keep-alive / non-JSON frames */
    }
  });

  return {
    clear,
    resume: () => {
      paused.set(false);
      newCount.set(0);
      const batch = incoming;
      incoming = [];
      stick = true;
      append(batch);
    },
    onScroll: () => {
      const el = scroller.el;
      if (el) stick = el.scrollTop <= 8; // near the top → follow the newest rows
    },
  };
}

/**
 * The toolbar: filter input, status/auth selects, count, and pause/resume.
 * @param {LogState} s
 * @returns {TemplateResult}
 */
function Toolbar(s: LogState): TemplateResult {
  const { filter, statusFilter, authFilter, visible, events, paused, newCount, resume, clear } = s;
  const statusOpts = [
    { value: "all", label: "All status" },
    { value: "2", label: "2xx" },
    { value: "3", label: "3xx" },
    { value: "4", label: "4xx" },
    { value: "5", label: "5xx" },
  ];
  const authOpts = [
    { value: "any", label: "Any auth" },
    { value: "header", label: "Header" },
    { value: "presigned", label: "Presigned" },
    { value: "anonymous", label: "Anonymous" },
  ];
  const pause = () => {
    if (paused.val) resume();
    else paused.set(true);
  };
  // Drain the server ring (durable across reconnect + consistent across tabs),
  // then empty this view immediately. The server's `clear` frame also lands and
  // empties every open tab, including this one.
  const onClear = () => {
    void clearEvents();
    clear();
  };
  return html`
    <div class="toolbar split align-center pad-md border-b">
      <div class="cluster align-center gap-md">
        <div class="toolbar-filter">
          ${Input({ value: filter, placeholder: "Filter by op, key, method", size: "sm" })}
        </div>
        ${Select({ value: statusFilter, options: statusOpts, size: "sm" })}
        ${Select({ value: authFilter, options: authOpts, size: "sm" })}
      </div>
      <div class="cluster align-center gap-md">
        <span class="count mono">${() => `${visible.val.length} / ${events.val.length}`}</span>
        <button class="clear-btn cluster align-center" @click=${onClear} title="Clear log" aria-label="Clear log">${TrashIcon()}</button>
        <button
          class=${() => "pause-btn cluster align-center gap-xs" + (paused.val ? " paused" : "")}
          @click=${pause}
          title=${() => (paused.val ? "Resume live tail" : "Pause live tail")}
          aria-label=${() => (paused.val ? "Resume live tail" : "Pause live tail")}
        >
          ${() => (paused.val ? PlayIcon() : PauseIcon())}
          ${() => (paused.val && newCount.val > 0 ? html`<span class="pause-count mono">${newCount}</span>` : "")}
        </button>
      </div>
    </div>
  `;
}

/**
 * The log table. Rows are keyed by event id; a click expands the detail row.
 * @param {LogState} s
 * @returns {TemplateResult}
 */
function LogTable(s: LogState): TemplateResult {
  return html`
    <table class="log-table">
      <thead>
        <tr>
          <th class="c-time text-start">TIME</th>
          <th class="c-method text-start">METHOD</th>
          <th class="c-op text-start">OPERATION</th>
          <th class="c-key text-start">BUCKET / KEY</th>
          <th class="c-status text-start">STATUS</th>
          <th class="c-dur text-start">DUR</th>
          <th class="c-bytes text-start">BYTES</th>
        </tr>
      </thead>
      <tbody>
        ${each(
          s.visible as unknown as Signal<LogEvent[]>,
          (e) => Row(e, s.expanded, s.now),
          (e) => e.id,
        )}
      </tbody>
    </table>
  `;
}

/**
 * One log row plus its (conditionally rendered) detail row.
 * @returns {TemplateResult}
 */
function Row(e: LogEvent, expanded: Signal<number | null>, now: Signal<number>): TemplateResult {
  const toggle = () => expanded.update((id) => (id === e.id ? null : e.id));
  const durCls = "c-dur" + (e.duration_ms >= SLOW_MS ? " slow" : "");
  return html`
    <tr class="log-row" @click=${toggle}>
      <td class="c-time mono">${() => timeAgo(e.ts, now.val)}</td>
      <td class="c-method"><span class=${"method m-" + e.method.toLowerCase()}>${e.method}</span></td>
      <td class="c-op">${e.op ?? "—"}</td>
      <td class="c-key mono" title=${targetOf(e)}>${targetOf(e)}</td>
      <td class="c-status">
        <span class=${"pill s-" + statusClass(e.status)}>${e.status}</span>
      </td>
      <td class=${durCls + " mono"}>${e.duration_ms} ms</td>
      <td class="c-bytes mono">${bytesCell(e)}</td>
    </tr>
    <tr class=${() => "log-detail" + (expanded.val === e.id ? " open" : "")}>
      ${() => (expanded.val === e.id ? Detail(e) : "")}
    </tr>
  `;
}

/**
 * The expanded detail cell for a row.
 * @returns {TemplateResult}
 */
function Detail(e: LogEvent): TemplateResult {
  const field = (k: string, v: string) =>
    html`<div class="kv"><span class="k">${k}</span><span class="v mono">${v}</span></div>`;
  return html`
    <td colspan="7">
      <div class="detail-grid grid">
        ${field("time", new Date(e.ts).toISOString())}
        ${field("op", e.op ?? "—")}
        ${field("auth", e.auth)}
        ${field("error_code", e.error_code ?? "—")}
        ${field("bytes_in", String(e.bytes_in))}
        ${field("bytes_out", String(e.bytes_out))}
        ${field("duration", e.duration_ms + " ms")}
        ${field("id", String(e.id))}
      </div>
      ${JumpToObject(e)}
    </td>
  `;
}

/**
 * A "View object" deep link for a row that names a concrete object (both a
 * bucket and a key). A same-origin `<a href>` — zero intercepts the click and
 * routes into the bucket browser's object detail. Rows without a key
 * (ListBuckets, CreateBucket, pre-resolution errors) render nothing.
 * @param {LogEvent} e
 * @returns {TemplateResult | string}
 */
function JumpToObject(e: LogEvent): TemplateResult | string {
  if (!e.bucket || !e.key) return "";
  const href = locationToUrl({ bucket: e.bucket, prefix: parentPrefix(e.key), object: e.key });
  return html`<a class="detail-jump" href=${href}>View object →</a>`;
}
