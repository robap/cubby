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
 * A human "time-ago" label for an event's wall-clock `ts` relative to `now`
 * (both Unix ms): `now` under a second, then `5s`, `2m`, `1h`, `3d`. Coarse and
 * monotonic — it stays readable as the log ages, unlike seconds-since-first.
 * @param {number} ts
 * @param {number} now
 * @returns {string}
 */
export function timeAgo(ts: number, now: number): string {
  const secs = Math.max(0, Math.floor((now - ts) / 1000));
  if (secs < 1) return "now";
  if (secs < 60) return `${secs}s`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h`;
  return `${Math.floor(hours / 24)}d`;
}
