// Live request log — the home screen. Subscribes to the SSE stream, batches
// incoming events per animation frame, and renders a dense, colour-coded table
// with filter / pause / auto-scroll / click-to-expand.

import { computed, each, effect, html, ref, signal } from "zero";
import type { Signal, TemplateResult } from "zero";
import { Input, Select } from "zero/components";
import type { LogEvent } from "../lib/api.ts";
import { bytesCell, statusClass, targetOf } from "../lib/format.ts";
import { appendCapped, elapsedLabel, matchesFilter } from "../lib/log.ts";

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
  origin: () => number;
  resume: () => void;
  onScroll: () => void;
};

/**
 * @returns {TemplateResult}
 */
export default function LiveLog(): TemplateResult {
  const s = createLogState();
  return html`
    <section class="screen log-screen flex-col">
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

  const ingest = createIngest(events, paused, newCount, scroller);
  const visible = computed(() =>
    events.val.filter((e) => matchesFilter(e, filter.val, statusFilter.val, authFilter.val)),
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
    origin: ingest.origin,
    resume: ingest.resume,
    onScroll: ingest.onScroll,
  };
}

/**
 * Wire the SSE stream to `events` with per-animation-frame batching, pause
 * buffering, and auto-scroll bookkeeping. Returns the imperative handles the
 * view needs (`origin`, `resume`, `onScroll`).
 * @returns {{ origin: () => number, resume: () => void, onScroll: () => void }}
 */
function createIngest(
  events: Signal<LogEvent[]>,
  paused: Signal<boolean>,
  newCount: Signal<number>,
  scroller: ReturnType<typeof ref<HTMLElement>>,
): { origin: () => number; resume: () => void; onScroll: () => void } {
  let origin = 0; // first event's ts, for the relative TIME column
  let stick = true; // pinned to bottom → auto-scroll follows new rows
  let incoming: LogEvent[] = []; // buffered until the next frame / resume
  let rafScheduled = false;

  const append = (batch: LogEvent[]) => {
    if (batch.length === 0) return;
    if (origin === 0 && batch[0]) origin = batch[0].ts;
    events.update((prev) => appendCapped(prev, batch, MAX_ROWS));
    if (stick && !paused.val) {
      requestAnimationFrame(() => {
        if (scroller.el) scroller.el.scrollTop = scroller.el.scrollHeight;
      });
    }
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

  effect(() => {
    const es = new EventSource("/_/api/events");
    es.onmessage = (msg) => {
      try {
        const parsed = JSON.parse(msg.data) as LogEvent;
        if (typeof parsed.id === "number") {
          incoming.push(parsed);
          schedule();
        }
      } catch {
        /* ignore keep-alive / non-JSON frames */
      }
    };
    return () => es.close();
  });

  return {
    origin: () => origin,
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
      if (el) stick = el.scrollTop + el.clientHeight >= el.scrollHeight - 8;
    },
  };
}

/**
 * The toolbar: filter input, status/auth selects, count, and pause/resume.
 * @param {LogState} s
 * @returns {TemplateResult}
 */
function Toolbar(s: LogState): TemplateResult {
  const { filter, statusFilter, authFilter, visible, events, paused, newCount, resume } = s;
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
        <button class=${() => "pause-btn" + (paused.val ? " paused" : "")} @click=${pause}>
          ${() => (paused.val ? `▶ ${newCount.val} new` : "❚❚ Pause")}
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
          (e) => Row(e, s.expanded, s.origin()),
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
function Row(e: LogEvent, expanded: Signal<number | null>, origin: number): TemplateResult {
  const elapsed = elapsedLabel(e.ts, origin);
  const toggle = () => expanded.update((id) => (id === e.id ? null : e.id));
  const durCls = "c-dur" + (e.duration_ms >= SLOW_MS ? " slow" : "");
  return html`
    <tr class="log-row" @click=${toggle}>
      <td class="c-time mono">${elapsed}</td>
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
        ${field("op", e.op ?? "—")}
        ${field("auth", e.auth)}
        ${field("error_code", e.error_code ?? "—")}
        ${field("bytes_in", String(e.bytes_in))}
        ${field("bytes_out", String(e.bytes_out))}
        ${field("duration", e.duration_ms + " ms")}
        ${field("id", String(e.id))}
      </div>
    </td>
  `;
}
