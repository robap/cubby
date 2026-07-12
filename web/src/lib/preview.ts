// Object-detail helpers: decide how to preview an object from its content-type
// and size, and the presign expiry choices. Pure and unit-testable — the view
// only maps these onto DOM.

/** How the object-detail pane should render an object's bytes. */
export type PreviewKind = "image" | "text" | "json" | "none";

/** Above this size, text/JSON is not inlined (download only). Images stream. */
export const PREVIEW_MAX_BYTES = 2 * 1024 * 1024; // 2 MB

/** Textual, non-JSON content-types worth rendering as monospace text. */
const TEXT_TYPES = new Set([
  "application/xml",
  "application/javascript",
  "application/x-javascript",
  "application/x-sh",
]);

/**
 * Classify an object for inline preview. Images always preview (they stream
 * into an `<img>`); text/JSON preview only under `PREVIEW_MAX_BYTES`; anything
 * else is download-only.
 * @param {string | null} contentType
 * @param {number} size
 * @returns {PreviewKind}
 */
export function previewKind(contentType: string | null, size: number): PreviewKind {
  const ct = (contentType ?? "").toLowerCase().split(";")[0]?.trim() ?? "";
  if (ct.startsWith("image/")) return "image";
  const textual =
    ct === "application/json" ||
    ct.endsWith("+json") ||
    ct.startsWith("text/") ||
    TEXT_TYPES.has(ct);
  if (!textual) return "none";
  if (size > PREVIEW_MAX_BYTES) return "none";
  return ct === "application/json" || ct.endsWith("+json") ? "json" : "text";
}

/** One presign-expiry choice. */
export type ExpiryOption = { label: string; seconds: number };

/** The expiry-picker choices, per the mockup (5 min / 1 h / 24 h / 7 days). */
export const EXPIRY_OPTIONS: ExpiryOption[] = [
  { label: "5 minutes", seconds: 300 },
  { label: "1 hour", seconds: 3600 },
  { label: "24 hours", seconds: 86400 },
  { label: "7 days", seconds: 604800 },
];
