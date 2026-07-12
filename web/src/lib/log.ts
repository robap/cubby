// Pure logic for the live request log, extracted from the view so it is unit
// testable: the toolbar filter predicate, the capped ring append, and the
// relative-time label.

import type { LogEvent } from "./api.ts";
import { targetOf } from "./format.ts";

/**
 * Whether an event passes the toolbar's filter text, status class, and auth.
 * `status` is `"all"` or a leading digit (`"2"`..`"5"`); `auth` is `"any"` or an
 * auth kind. The text matches method / op / bucketKey, case-insensitively.
 * @param {LogEvent} e
 * @param {string} filter
 * @param {string} status
 * @param {string} auth
 * @returns {boolean}
 */
export function matchesFilter(e: LogEvent, filter: string, status: string, auth: string): boolean {
  if (status !== "all" && Math.floor(e.status / 100) !== Number(status)) return false;
  if (auth !== "any" && e.auth !== auth) return false;
  const q = filter.trim().toLowerCase();
  if (!q) return true;
  const hay = `${e.method} ${e.op ?? ""} ${targetOf(e)}`.toLowerCase();
  return hay.includes(q);
}

/**
 * Append `batch` to `prev`, retaining only the most recent `max` events. An
 * empty batch returns `prev` unchanged (same reference).
 * @param {LogEvent[]} prev
 * @param {LogEvent[]} batch
 * @param {number} max
 * @returns {LogEvent[]}
 */
export function appendCapped(prev: LogEvent[], batch: LogEvent[], max: number): LogEvent[] {
  if (batch.length === 0) return prev;
  const next = prev.concat(batch);
  return next.length > max ? next.slice(next.length - max) : next;
}

/**
 * The relative TIME label: seconds since `origin`, two decimals, clamped at 0.
 * @param {number} ts
 * @param {number} origin
 * @returns {string}
 */
export function elapsedLabel(ts: number, origin: number): string {
  const secs = Math.max(0, (ts - origin) / 1000);
  return `${secs.toFixed(2)}s`;
}
