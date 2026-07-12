// Presentation helpers shared across the screens: byte sizes, durations,
// status classes, key truncation. Pure functions, no reactivity.

/**
 * @typedef {import("../lib/api.ts").LogEvent} LogEvent
 */

/**
 * Human-readable byte size, e.g. `2.4 MB`, `512 B`.
 * @param {number} n
 * @returns {string}
 */
export function humanBytes(n: number): string {
  if (!Number.isFinite(n) || n < 0) return "—";
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB", "PB"];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i += 1;
  }
  return `${v.toFixed(1)} ${units[i]}`;
}

/**
 * Group an integer with thousands separators (`984221` → `984,221`). Explicit,
 * not `toLocaleString`, so grouping is stable regardless of the JS engine's
 * `Intl` support.
 * @param {number} n
 * @returns {string}
 */
export function groupDigits(n: number): string {
  return String(n).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

/**
 * The status class used for coloring: 2xx ok, 3xx redirect, 4xx warn, 5xx err.
 * @param {number} status
 * @returns {"ok" | "redirect" | "warn" | "err"}
 */
export function statusClass(status: number): "ok" | "redirect" | "warn" | "err" {
  if (status >= 500) return "err";
  if (status >= 400) return "warn";
  if (status >= 300) return "redirect";
  return "ok";
}

/**
 * The bytes cell for the log: `↑` for request bytes, `↓` for response bytes.
 * @param {LogEvent} e
 * @returns {string}
 */
export function bytesCell(e: { bytes_in: number; bytes_out: number }): string {
  if (e.bytes_in > 0) return `↑ ${humanBytes(e.bytes_in)}`;
  if (e.bytes_out > 0) return `↓ ${humanBytes(e.bytes_out)}`;
  return "—";
}

/**
 * `bucket/key`, `bucket`, or `—` for a log row's target.
 * @param {{ bucket: string | null, key: string | null }} e
 * @returns {string}
 */
export function targetOf(e: { bucket: string | null; key: string | null }): string {
  if (e.bucket && e.key) return `${e.bucket}/${e.key}`;
  if (e.bucket) return e.bucket;
  return "—";
}

/**
 * Middle-truncate a long key so both ends stay visible (`app…/hero.png`).
 * @param {string} s
 * @param {number} max
 * @returns {string}
 */
export function middleTruncate(s: string, max = 48): string {
  if (s.length <= max) return s;
  const half = Math.floor((max - 1) / 2);
  return `${s.slice(0, half)}…${s.slice(s.length - half)}`;
}

/**
 * Format an ISO-8601 timestamp as `YYYY-MM-DD HH:MM` (local), or `—`.
 * @param {string | null | undefined} iso
 * @returns {string}
 */
export function fmtDate(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "—";
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/**
 * The last path segment of a key (the "file name"), or the key itself.
 * @param {string} key
 * @returns {string}
 */
export function baseName(key: string): string {
  const trimmed = key.endsWith("/") ? key.slice(0, -1) : key;
  const idx = trimmed.lastIndexOf("/");
  return idx >= 0 ? trimmed.slice(idx + 1) : trimmed;
}
