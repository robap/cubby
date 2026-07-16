// Store for the bucket browser: the selected bucket + prefix (folder view), the
// flat key-search state, and the opened-object detail sub-view. Module-level
// signals are the store; components read them and mutate only via the exported
// mutators below. IO goes through the `/_/api/*` seam client in `lib/api.ts`.

import { navigate, route, signal } from "zero";
import type { BucketInfo, FolderView, ObjectMeta, SearchResult } from "../lib/api.ts";
import {
  createBucket as apiCreateBucket,
  deleteObject,
  getMeta,
  listBuckets,
  listObjects,
  presign,
  search,
  uploadObject,
} from "../lib/api.ts";
import type { BrowseLocation } from "../lib/browse.ts";
import { locationToUrl, parentPrefix, uploadKey, urlToLocation } from "../lib/browse.ts";
import { loadHealth } from "./chrome.ts";

/** All buckets, for the middle column. */
export const buckets = signal<BucketInfo[]>([]);
/** The bucket whose contents the main pane shows, or `null` before load. */
export const selectedBucket = signal<string | null>(null);
/** The current folder prefix within `selectedBucket` (`""` = bucket root). */
export const prefix = signal<string>("");
/** The folder listing for `selectedBucket` + `prefix`. */
export const folder = signal<FolderView | null>(null);

/** The live search term (blank = folder browsing). */
export const searchTerm = signal<string>("");
/** Whether search spans every bucket (vs. just `selectedBucket`). */
export const allBuckets = signal<boolean>(false);
/** The latest flat search results, or `null` when not searching. */
export const searchResults = signal<SearchResult | null>(null);

/** The opened object's key (drives the detail sub-view), or `null`. */
export const selectedObject = signal<string | null>(null);
/** Metadata for `selectedObject`. */
export const objectMeta = signal<ObjectMeta | null>(null);
/** The most recently minted presigned URL for the open object. */
export const presignedUrl = signal<string | null>(null);

/**
 * Load the bucket list for the middle column. Selection is driven by the URL
 * (see {@link applyLocation}), so this no longer auto-selects a bucket.
 * @returns {Promise<void>}
 */
export async function loadBuckets(): Promise<void> {
  const res = await listBuckets();
  buckets.set(res.buckets);
}

/**
 * Push a browser location into the address bar so it is linkable and drives
 * Back/Forward. Guarded because `navigate` needs a running app: in unit tests
 * (no live router) the store's own signals still drive the view.
 * @param {BrowseLocation} loc
 * @param {boolean} [replace] Replace the history entry (URL normalization).
 * @returns {void}
 */
function pushLocation(loc: BrowseLocation, replace = false): void {
  try {
    navigate(locationToUrl(loc), replace ? { replace: true } : undefined);
  } catch {
    /* no running app (unit tests) — the store signals still drive the view */
  }
}

/**
 * Create a bucket, then refresh the list, select it, and update the counts.
 * Rejects (HttpError) if the name is invalid (400) or already exists (409) —
 * the caller surfaces the message.
 * @param {string} name
 * @returns {Promise<void>}
 */
export async function createBucket(name: string): Promise<void> {
  await apiCreateBucket(name);
  await loadBuckets();
  await selectBucket(name);
  await loadHealth();
}

/**
 * Select a bucket: reset to its root, clear search + open object, load folder,
 * and write the URL.
 * @param {string} name
 * @returns {Promise<void>}
 */
export async function selectBucket(name: string): Promise<void> {
  selectedBucket.set(name);
  prefix.set("");
  searchTerm.set("");
  searchResults.set(null);
  selectedObject.set(null);
  objectMeta.set(null);
  presignedUrl.set(null);
  pushLocation({ bucket: name, prefix: "", object: null });
  await loadFolder();
}

/**
 * Navigate to a folder prefix within the current bucket, load it, and write the
 * URL.
 * @param {string} nextPrefix
 * @returns {Promise<void>}
 */
export async function navigateTo(nextPrefix: string): Promise<void> {
  prefix.set(nextPrefix);
  selectedObject.set(null);
  objectMeta.set(null);
  presignedUrl.set(null);
  pushLocation({ bucket: selectedBucket.val, prefix: nextPrefix, object: null });
  await loadFolder();
}

/**
 * Load the folder view for the current bucket + prefix.
 * @returns {Promise<void>}
 */
export async function loadFolder(): Promise<void> {
  const bucket = selectedBucket.val;
  if (!bucket) return;
  folder.set(await listObjects(bucket, prefix.val));
}

/**
 * Set the search term. Blank clears results (back to folder browse); otherwise
 * runs the flat substring search.
 * @param {string} term
 * @returns {Promise<void>}
 */
export async function setSearch(term: string): Promise<void> {
  searchTerm.set(term);
  if (term.trim().length === 0) {
    searchResults.set(null);
    return;
  }
  await runSearch();
}

/**
 * Toggle the "all buckets" scope and re-run any active search.
 * @returns {Promise<void>}
 */
export async function toggleAllBuckets(): Promise<void> {
  allBuckets.set(!allBuckets.val);
  if (searchTerm.val.trim().length > 0) await runSearch();
}

/**
 * Run the flat key search for the current term + scope.
 * @returns {Promise<void>}
 */
export async function runSearch(): Promise<void> {
  const scope = allBuckets.val ? null : selectedBucket.val;
  searchResults.set(await search(searchTerm.val, scope));
}

/**
 * Open an object's detail sub-view: point the browser at the object's bucket +
 * folder, record the key, write the URL, and load its metadata. Setting the
 * bucket/prefix makes a cross-bucket search hit (and a copy-pasted deep link)
 * resolve correctly and lets the back-crumb return to its folder.
 * @param {string} bucket
 * @param {string} key
 * @returns {Promise<void>}
 */
export async function openObject(bucket: string, key: string): Promise<void> {
  selectedBucket.set(bucket);
  prefix.set(parentPrefix(key));
  selectedObject.set(key);
  objectMeta.set(null);
  presignedUrl.set(null);
  pushLocation({ bucket, prefix: parentPrefix(key), object: key });
  objectMeta.set(await getMeta(bucket, key));
}

/** Close the object detail sub-view, back to the listing (and its URL). */
export function closeObject(): void {
  selectedObject.set(null);
  objectMeta.set(null);
  presignedUrl.set(null);
  pushLocation({ bucket: selectedBucket.val, prefix: prefix.val, object: null });
}

/**
 * Upload files into the current prefix, then refresh the folder + counts.
 * @param {Iterable<File>} files
 * @returns {Promise<void>}
 */
export async function uploadFiles(files: Iterable<File>): Promise<void> {
  const bucket = selectedBucket.val;
  if (!bucket) return;
  for (const file of files) {
    await uploadObject(bucket, uploadKey(prefix.val, file.name), file);
  }
  await loadFolder();
  await loadBuckets();
  await loadHealth(); // refresh the nav-footer counts after a UI mutation
}

/**
 * Delete an object, then refresh the folder + counts.
 * @param {string} key
 * @returns {Promise<void>}
 */
export async function removeObject(key: string): Promise<void> {
  const bucket = selectedBucket.val;
  if (!bucket) return;
  await deleteObject(bucket, key);
  await loadFolder();
  await loadBuckets();
  await loadHealth(); // refresh the nav-footer counts after a UI mutation
}

/**
 * Mint a presigned URL for the open object and stash it for display.
 * @param {"GET" | "PUT"} method
 * @param {number} expiresInS
 * @returns {Promise<void>}
 */
export async function generatePresign(method: "GET" | "PUT", expiresInS: number): Promise<void> {
  const bucket = selectedBucket.val;
  const key = selectedObject.val;
  if (!bucket || !key) return;
  const res = await presign({ method, bucket, key, expires_in_s: expiresInS });
  presignedUrl.set(res.url);
}

/**
 * Hydrate the store from a browser location — the single loader for the folder
 * and object detail (cold load, deep link, Back/Forward). Diffs the desired
 * location against the current store and only fetches what changed, so an
 * in-app mutator (which already loaded and then pushed the URL) leaves this a
 * no-op rather than double-fetching. An empty location (no bucket) normalizes
 * the URL to the first bucket.
 * @param {BrowseLocation} loc
 * @returns {Promise<void>}
 */
export async function applyLocation(loc: BrowseLocation): Promise<void> {
  if (!loc.bucket) {
    if (buckets.val.length === 0) await loadBuckets();
    const first = buckets.val[0];
    if (first) pushLocation({ bucket: first.name, prefix: "", object: null }, true);
    return;
  }
  const bucketChanged = selectedBucket.val !== loc.bucket;
  const prefixChanged = prefix.val !== loc.prefix;
  if (bucketChanged) {
    searchTerm.set("");
    searchResults.set(null);
  }
  selectedBucket.set(loc.bucket);
  prefix.set(loc.prefix);
  if (bucketChanged || prefixChanged || folder.val === null) {
    await loadFolder();
  }
  if (loc.object === null) {
    selectedObject.set(null);
    objectMeta.set(null);
    presignedUrl.set(null);
  } else if (selectedObject.val !== loc.object || objectMeta.val === null) {
    selectedObject.set(loc.object);
    objectMeta.set(null);
    presignedUrl.set(null);
    objectMeta.set(await getMeta(loc.bucket, loc.object));
  }
}

/**
 * React to a `/_/browser` URL: decode the current location and hydrate.
 *
 * The query is read *inside* the microtask — from `route()`, fresh — rather than
 * captured at call time. zero's effects are synchronous and its router sets
 * `path`, `params`, and `query` as separate signals, so the route effect can
 * fire on the `path` change while `query` is still the *previous* route's value
 * (a torn read). Capturing that stale query would send a bogus empty-bucket
 * location through {@link applyLocation}, whose normalize branch would clobber
 * the real destination. Deferring the read to the microtask lets every
 * synchronous set in one navigation settle first, so we always hydrate from the
 * consistent, committed URL. The deferral also keeps the effect tracking only
 * `route()` (never the store signals this reads), so it can't loop.
 * @returns {void}
 */
export function syncBrowseFromUrl(): void {
  queueMicrotask(() => {
    let query: Record<string, string>;
    try {
      const r = route();
      if (r.path !== "/_/browser") return; // navigated away before we ran
      query = r.query;
    } catch {
      return; // no running app (unit tests drive applyLocation directly)
    }
    void applyLocation(urlToLocation(query));
  });
}
