// Store for the per-bucket Notifications panel: whether the panel is open, the
// destinations for the current bucket, and the mutators that load/add/remove
// them. Module-level signals are the store; components read them and mutate only
// via the exported mutators. IO goes through the `/_/api/*` seam client.

import { signal } from "zero";
import type { NotificationDraft, NotificationInfo } from "../lib/api.ts";
import {
  createNotification,
  deleteNotification,
  listNotifications,
} from "../lib/api.ts";

/** Whether the Notifications panel is showing (vs. the folder/search view). */
export const panelOpen = signal<boolean>(false);
/** The destinations for the bucket the panel was last loaded for. */
export const notifications = signal<NotificationInfo[]>([]);
/** The bucket whose destinations are currently loaded. */
export const loadedBucket = signal<string | null>(null);

/**
 * Open the Notifications panel for `bucket` and load its destinations.
 * @param {string} bucket
 * @returns {Promise<void>}
 */
export async function openPanel(bucket: string): Promise<void> {
  panelOpen.set(true);
  await load(bucket);
}

/** Close the Notifications panel (back to the folder/search view). */
export function closePanel(): void {
  panelOpen.set(false);
}

/**
 * Load the destinations for `bucket` into the panel.
 * @param {string} bucket
 * @returns {Promise<void>}
 */
export async function load(bucket: string): Promise<void> {
  loadedBucket.set(bucket);
  const res = await listNotifications(bucket);
  notifications.set(res.notifications);
}

/**
 * Add a destination to `bucket`, then refresh the list. Rejects (HttpError 400)
 * on an invalid destination — the caller surfaces the seam's message.
 * @param {string} bucket
 * @param {NotificationDraft} draft
 * @returns {Promise<void>}
 */
export async function add(bucket: string, draft: NotificationDraft): Promise<void> {
  await createNotification(bucket, draft);
  await load(bucket);
}

/**
 * Remove a destination by id from `bucket`, then refresh the list.
 * @param {string} bucket
 * @param {number} id
 * @returns {Promise<void>}
 */
export async function remove(bucket: string, id: number): Promise<void> {
  await deleteNotification(bucket, id);
  await load(bucket);
}
