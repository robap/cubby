// Pure logic for the bucket browser, extracted from the view for unit testing:
// breadcrumb building, folder label derivation, upload key composition, and
// search-term highlighting.

/** Which view the browser's main pane shows for a given search term. */
export type ViewMode = "folder" | "search";

/**
 * The main-pane mode: a non-blank search term flips folder-browse to the flat
 * search list; clearing it (empty/whitespace) returns to folder-browse.
 * @param {string} searchTerm
 * @returns {ViewMode}
 */
export function viewMode(searchTerm: string): ViewMode {
  return searchTerm.trim().length > 0 ? "search" : "folder";
}

/** One breadcrumb: a display label and the prefix it navigates to. */
export type Crumb = { label: string; prefix: string };

/**
 * Breadcrumb trail for a bucket + prefix: the bucket root, then one crumb per
 * prefix segment, each carrying the cumulative prefix.
 * @param {string} bucket
 * @param {string} prefix
 * @returns {Crumb[]}
 */
export function crumbs(bucket: string, prefix: string): Crumb[] {
  const out: Crumb[] = [{ label: bucket, prefix: "" }];
  const segments = prefix.split("/").filter((s) => s.length > 0);
  let acc = "";
  for (const seg of segments) {
    acc += `${seg}/`;
    out.push({ label: seg, prefix: acc });
  }
  return out;
}

/**
 * The display label for a folder (common prefix) under the current prefix:
 * strip the current prefix, keep the trailing slash.
 * @param {string} commonPrefix
 * @param {string} currentPrefix
 * @returns {string}
 */
export function folderLabel(commonPrefix: string, currentPrefix: string): string {
  return commonPrefix.startsWith(currentPrefix)
    ? commonPrefix.slice(currentPrefix.length)
    : commonPrefix;
}

/**
 * The key a dropped/picked file uploads to: the current prefix + file name.
 * @param {string} prefix
 * @param {string} fileName
 * @returns {string}
 */
export function uploadKey(prefix: string, fileName: string): string {
  return `${prefix}${fileName}`;
}

/** A browser location: the selected bucket, the folder prefix, and the open
 * object key (or `null` when browsing a folder rather than an object). */
export type BrowseLocation = {
  bucket: string | null;
  prefix: string;
  object: string | null;
};

/**
 * The parent folder prefix of a key: everything up to and including its last
 * `/`, or `""` for a top-level key. So an open object hydrates back into the
 * folder that contains it.
 * @param {string} key
 * @returns {string}
 */
export function parentPrefix(key: string): string {
  const idx = key.lastIndexOf("/");
  return idx >= 0 ? key.slice(0, idx + 1) : "";
}

/**
 * Encode a browser location as a `/_/browser` URL. A folder carries
 * `?bucket=&prefix=`, an open object `?bucket=&object=` (its prefix is derived
 * from the key on the way back). Values are percent-encoded with `encodeURIComponent`
 * so keys with `/` and spaces round-trip as `%20`, never a `+`. No bucket yet
 * → the bare browser URL (default landing).
 * @param {BrowseLocation} loc
 * @returns {string}
 */
export function locationToUrl(loc: BrowseLocation): string {
  if (!loc.bucket) return "/_/browser";
  const parts = [`bucket=${encodeURIComponent(loc.bucket)}`];
  if (loc.object !== null) {
    parts.push(`object=${encodeURIComponent(loc.object)}`);
  } else if (loc.prefix) {
    parts.push(`prefix=${encodeURIComponent(loc.prefix)}`);
  }
  return `/_/browser?${parts.join("&")}`;
}

/**
 * Decode a browser location from a router query map (values already
 * percent-decoded). An `object` query wins over `prefix` and derives its own
 * folder prefix; a `prefix` query without an object is a folder view; no bucket
 * is the default landing.
 * @param {Record<string, string>} query
 * @returns {BrowseLocation}
 */
export function urlToLocation(query: Record<string, string>): BrowseLocation {
  const bucket = query.bucket ?? null;
  if (!bucket) return { bucket: null, prefix: "", object: null };
  const object = query.object ?? null;
  const prefix = object !== null ? parentPrefix(object) : query.prefix ?? "";
  return { bucket, prefix, object };
}

/** One run of a key, flagged as a search match or not. */
export type HighlightPart = { text: string; match: boolean };

/**
 * Split `key` into runs around each case-insensitive occurrence of `term`, so
 * the matched runs can be highlighted. An empty or absent term yields one
 * unmatched run.
 * @param {string} key
 * @param {string} term
 * @returns {HighlightPart[]}
 */
export function highlightParts(key: string, term: string): HighlightPart[] {
  if (!term) return [{ text: key, match: false }];
  const needle = term.toLowerCase();
  const hay = key.toLowerCase();
  const parts: HighlightPart[] = [];
  let i = 0;
  let at = hay.indexOf(needle, i);
  while (at !== -1) {
    if (at > i) parts.push({ text: key.slice(i, at), match: false });
    parts.push({ text: key.slice(at, at + needle.length), match: true });
    i = at + needle.length;
    at = hay.indexOf(needle, i);
  }
  if (i < key.length) parts.push({ text: key.slice(i), match: false });
  return parts.length > 0 ? parts : [{ text: key, match: false }];
}
