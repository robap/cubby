// Object-detail helpers: decide how to preview an object from its content-type
// and size, and the presign expiry choices. Pure and unit-testable — the view
// only maps these onto DOM.

/** How the object-detail pane should render an object's bytes. */
export type PreviewKind = "image" | "text" | "json" | "xml" | "none";

/** Above this size, text/JSON/XML is not inlined (download only). Images stream. */
export const PREVIEW_MAX_BYTES = 2 * 1024 * 1024; // 2 MB

/** Textual content-types worth rendering as monospace text. */
const TEXT_TYPES = new Set([
  "application/javascript",
  "application/x-javascript",
  "application/x-sh",
]);

/**
 * Classify an object for inline preview. Images always preview (they stream
 * into an `<img>`); text/JSON/XML preview only under `PREVIEW_MAX_BYTES`;
 * anything else is download-only. JSON and XML get their own kinds so the pane
 * can pretty-print them.
 * @param {string | null} contentType
 * @param {number} size
 * @returns {PreviewKind}
 */
export function previewKind(contentType: string | null, size: number): PreviewKind {
  const ct = (contentType ?? "").toLowerCase().split(";")[0]?.trim() ?? "";
  if (ct.startsWith("image/")) return "image";
  const isJson = ct === "application/json" || ct.endsWith("+json");
  const isXml = ct === "application/xml" || ct === "text/xml" || ct.endsWith("+xml");
  const textual = isJson || isXml || ct.startsWith("text/") || TEXT_TYPES.has(ct);
  if (!textual) return "none";
  if (size > PREVIEW_MAX_BYTES) return "none";
  if (isJson) return "json";
  if (isXml) return "xml";
  return "text";
}

/**
 * Re-indent JSON so a minified blob is readable. Content that does not parse as
 * JSON is returned unchanged (never throws, never blanks the pane).
 * @param {string} raw
 * @returns {string}
 */
export function prettyJson(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}

/**
 * Indent XML so each element sits on its own line, nested by depth — a small
 * Node-free indenter (no library). Input that isn't well-formed (tags don't
 * balance, or it doesn't start with a tag) falls back to the raw text.
 * @param {string} raw
 * @returns {string}
 */
export function prettyXml(raw: string): string {
  const src = raw.trim();
  if (!src.startsWith("<")) return raw;
  const tokens = src.match(/<[^>]+>|[^<]+/g);
  if (!tokens) return raw;

  const out: string[] = [];
  const stack: string[] = [];
  let depth = 0;
  const pad = () => "  ".repeat(depth);
  for (const token of tokens) {
    const tok = token.trim();
    if (!tok) continue;
    if (tok.startsWith("<?") || tok.startsWith("<!")) {
      out.push(pad() + tok); // declaration / doctype / comment
    } else if (tok.startsWith("</")) {
      if (stack.pop() !== tagName(tok)) return raw; // mismatched → malformed
      depth = Math.max(0, depth - 1);
      out.push(pad() + tok);
    } else if (tok.endsWith("/>")) {
      out.push(pad() + tok); // self-closing
    } else if (tok.startsWith("<")) {
      out.push(pad() + tok);
      stack.push(tagName(tok));
      depth += 1;
    } else {
      out.push(pad() + tok); // text content
    }
  }
  if (stack.length > 0) return raw; // unclosed tags → malformed
  return out.join("\n");
}

/**
 * The element name of a `<tag …>` or `</tag>` token (up to the first space).
 * @param {string} tag
 * @returns {string}
 */
function tagName(tag: string): string {
  return tag.replace(/^<\/?/, "").replace(/\/?>$/, "").trim().split(/\s/)[0] ?? "";
}

/**
 * Pretty-print a preview body for its kind: JSON re-indented, XML indented,
 * plain text left as-is.
 * @param {PreviewKind} kind
 * @param {string} body
 * @returns {string}
 */
export function formatPreview(kind: PreviewKind, body: string): string {
  if (kind === "json") return prettyJson(body);
  if (kind === "xml") return prettyXml(body);
  return body;
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
