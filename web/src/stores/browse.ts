// Store for the bucket browser: the selected bucket + prefix (folder view), the
// flat key-search state, and the opened-object detail sub-view. Module-level
// signals are the store; components read them and mutate only via the exported
// mutators below. IO goes through the `/_/api/*` seam client in `lib/api.ts`.

import { signal } from "zero";
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
import { uploadKey } from "../lib/browse.ts";
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
 * Load the bucket list; select the first bucket if none is selected yet.
 * @returns {Promise<void>}
 */
export async function loadBuckets(): Promise<void> {
  const res = await listBuckets();
  buckets.set(res.buckets);
  if (selectedBucket.val === null && res.buckets.length > 0) {
    await selectBucket(res.buckets[0]!.name);
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
 * Select a bucket: reset to its root, clear search + open object, load folder.
 * @param {string} name
 * @returns {Promise<void>}
 */
export async function selectBucket(name: string): Promise<void> {
  selectedBucket.set(name);
  prefix.set("");
  searchTerm.set("");
  searchResults.set(null);
  selectedObject.set(null);
  await loadFolder();
}

/**
 * Navigate to a folder prefix within the current bucket and load it.
 * @param {string} nextPrefix
 * @returns {Promise<void>}
 */
export async function navigateTo(nextPrefix: string): Promise<void> {
  prefix.set(nextPrefix);
  selectedObject.set(null);
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
 * Open an object's detail sub-view: record the key and load its metadata.
 * @param {string} bucket
 * @param {string} key
 * @returns {Promise<void>}
 */
export async function openObject(bucket: string, key: string): Promise<void> {
  selectedObject.set(key);
  objectMeta.set(null);
  presignedUrl.set(null);
  objectMeta.set(await getMeta(bucket, key));
}

/** Close the object detail sub-view, back to the listing. */
export function closeObject(): void {
  selectedObject.set(null);
  objectMeta.set(null);
  presignedUrl.set(null);
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
