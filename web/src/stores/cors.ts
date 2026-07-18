// Store for the read-only per-bucket CORS panel: whether the panel is open, the
// current bucket's rules (or null = "no CORS configured"), and the load mutator.
// Display only — there is no add/edit/delete here; management is the real S3 API
// (PutBucketCors/DeleteBucketCors), the fidelity point. IO goes through the
// read-only `/_/api/buckets/{bucket}/cors` seam.

import { signal } from "zero";
import type { CorsInfo } from "../lib/api.ts";
import { getCors } from "../lib/api.ts";

/** Whether the CORS panel is showing (vs. the folder/search view). */
export const panelOpen = signal<boolean>(false);
/** The current bucket's rules, or `null` when it has no CORS configured. */
export const rules = signal<CorsInfo[] | null>(null);
/** The bucket whose rules are currently loaded. */
export const loadedBucket = signal<string | null>(null);

/**
 * Open the CORS panel for `bucket` and load its rules.
 * @param {string} bucket
 * @returns {Promise<void>}
 */
export async function openPanel(bucket: string): Promise<void> {
  panelOpen.set(true);
  await load(bucket);
}

/** Close the CORS panel (back to the folder/search view). */
export function closePanel(): void {
  panelOpen.set(false);
}

/**
 * Load the CORS rules for `bucket` into the panel. A bucket with no config sets
 * `rules` to `null` (the empty state).
 * @param {string} bucket
 * @returns {Promise<void>}
 */
export async function load(bucket: string): Promise<void> {
  loadedBucket.set(bucket);
  const res = await getCors(bucket);
  rules.set(res.cors);
}
