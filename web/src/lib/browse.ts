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
