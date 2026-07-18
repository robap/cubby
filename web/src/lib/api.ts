// The typed client for cubby's `/_/api/*` JSON seam. GET/POST/DELETE go through
// the zero HTTP client (JSON in, JSON out); binary upload uses `fetch` with the
// File as the raw body. Everything is mounted under `/_/`.

import { createHttp } from "zero/http";

// Resolve `fetch` at call time (not construction time) so the global stays the
// single source of truth — a test can swap `globalThis.fetch` and intercept the
// whole seam, and any polyfill takes effect uniformly.
const http = createHttp({
  fetch: (...args: Parameters<typeof fetch>) => globalThis.fetch(...args),
});

/** The shape of one live-log event (mirrors the Rust `Event`). */
export type LogEvent = {
  id: number;
  ts: number;
  method: string;
  op: string | null;
  bucket: string | null;
  key: string | null;
  status: number;
  duration_ms: number;
  bytes_in: number;
  bytes_out: number;
  auth: "header" | "presigned" | "anonymous";
  error_code: string | null;
};

export type Health = {
  status: string;
  version: string;
  uptime_s: number;
  data_dir: string;
  endpoint: string;
  region: string;
  bucket_count: number;
  object_count: number;
};

export type BucketInfo = {
  name: string;
  created_at: string;
  object_count: number;
  size: number;
};

export type ObjectInfo = {
  key: string;
  size: number;
  etag: string;
  last_modified: string;
};

export type FolderView = {
  prefix: string;
  delimiter: string | null;
  common_prefixes: string[];
  objects: ObjectInfo[];
  next_continuation_token: string | null;
};

export type SearchHit = {
  bucket: string;
  key: string;
  size: number;
  etag: string;
  last_modified: string;
};

export type SearchResult = {
  q: string;
  bucket: string | null;
  results: SearchHit[];
  truncated: boolean;
};

export type ObjectMeta = {
  key: string;
  size: number;
  etag: string;
  content_type: string | null;
  last_modified: string;
  storage_class: string;
  metadata: Record<string, string>;
};

export type PresignResult = { url: string; expires_at: string };

/** One webhook notification destination (mirrors the Rust `NotificationJson`). */
export type NotificationInfo = {
  id: number;
  url: string;
  events: string[];
  prefix: string | null;
  suffix: string | null;
  format: string;
  timeout_ms: number;
  created_at: string;
};

/** The body accepted by `POST …/notifications` (defaults applied server-side). */
export type NotificationDraft = {
  url: string;
  events: string[];
  prefix?: string;
  suffix?: string;
  format?: string;
  timeout_ms?: number;
};

/**
 * One CORS rule as the read-only seam returns it — the S3 `CORSRule` field names
 * (mirrors the Rust `cors::CorsRule` serialization), so the display speaks the
 * same vocabulary as `aws s3api get-bucket-cors`.
 */
export type CorsInfo = {
  AllowedOrigins: string[];
  AllowedMethods: string[];
  AllowedHeaders?: string[];
  ExposeHeaders?: string[];
  MaxAgeSeconds?: number;
  ID?: string;
};

/** Percent-encode a key for use in an API path (keeping it readable). */
function encKey(key: string): string {
  return key.split("/").map(encodeURIComponent).join("/");
}

/** `GET /_/api/health` */
export function getHealth(): Promise<Health> {
  return http.get<Health>("/_/api/health");
}

/** `POST /_/api/events/clear` — drain the server-side live-log ring. */
export function clearEvents(): Promise<unknown> {
  return http.post("/_/api/events/clear", {});
}

/** `GET /_/api/buckets` */
export function listBuckets(): Promise<{ buckets: BucketInfo[] }> {
  return http.get<{ buckets: BucketInfo[] }>("/_/api/buckets");
}

/** `POST /_/api/buckets` — create a bucket. Rejects (HttpError) on 400/409. */
export function createBucket(name: string): Promise<{ name: string }> {
  return http.post<{ name: string }>("/_/api/buckets", { name });
}

/** `GET /_/api/buckets/{bucket}/objects` — folder view for a prefix. */
export function listObjects(
  bucket: string,
  prefix: string,
  continuationToken?: string,
): Promise<FolderView> {
  const params = new URLSearchParams({ delimiter: "/", prefix });
  if (continuationToken) params.set("continuation-token", continuationToken);
  return http.get<FolderView>(`/_/api/buckets/${encodeURIComponent(bucket)}/objects?${params}`);
}

/** `GET /_/api/search` — flat substring key search. */
export function search(q: string, bucket?: string | null): Promise<SearchResult> {
  const params = new URLSearchParams({ q });
  if (bucket) params.set("bucket", bucket);
  return http.get<SearchResult>(`/_/api/search?${params}`);
}

/** `GET /_/api/buckets/{bucket}/objects/{key}` — object metadata. */
export function getMeta(bucket: string, key: string): Promise<ObjectMeta> {
  return http.get<ObjectMeta>(`/_/api/buckets/${encodeURIComponent(bucket)}/objects/${encKey(key)}`);
}

/** The URL that streams an object's bytes (for `<img>`, download, preview). */
export function contentUrl(bucket: string, key: string): string {
  return `/_/api/buckets/${encodeURIComponent(bucket)}/objects/${encKey(key)}?content`;
}

/** `PUT /_/api/buckets/{bucket}/objects/{key}` — upload a file's bytes. */
export async function uploadObject(bucket: string, key: string, file: Blob): Promise<void> {
  const res = await fetch(
    `/_/api/buckets/${encodeURIComponent(bucket)}/objects/${encKey(key)}`,
    { method: "PUT", body: file },
  );
  if (!res.ok) throw new Error(`upload failed: ${res.status}`);
}

/** `DELETE /_/api/buckets/{bucket}/objects/{key}` */
export function deleteObject(bucket: string, key: string): Promise<unknown> {
  return http.delete(`/_/api/buckets/${encodeURIComponent(bucket)}/objects/${encKey(key)}`);
}

/** `POST /_/api/presign` — mint a presigned URL. */
export function presign(body: {
  method: string;
  bucket: string;
  key: string;
  expires_in_s: number;
}): Promise<PresignResult> {
  return http.post<PresignResult>("/_/api/presign", body);
}

/** `GET /_/api/buckets/{bucket}/notifications` — the bucket's destinations. */
export function listNotifications(bucket: string): Promise<{ notifications: NotificationInfo[] }> {
  return http.get<{ notifications: NotificationInfo[] }>(
    `/_/api/buckets/${encodeURIComponent(bucket)}/notifications`,
  );
}

/**
 * `POST /_/api/buckets/{bucket}/notifications` — add a destination. Rejects
 * (HttpError 400) when the destination is invalid; the caller surfaces it.
 */
export function createNotification(
  bucket: string,
  draft: NotificationDraft,
): Promise<NotificationInfo> {
  return http.post<NotificationInfo>(
    `/_/api/buckets/${encodeURIComponent(bucket)}/notifications`,
    draft,
  );
}

/** `DELETE /_/api/buckets/{bucket}/notifications/{id}` — remove a destination. */
export function deleteNotification(bucket: string, id: number): Promise<unknown> {
  return http.delete(`/_/api/buckets/${encodeURIComponent(bucket)}/notifications/${id}`);
}

/**
 * `GET /_/api/buckets/{bucket}/cors` — the bucket's CORS rules (read-only), or
 * `cors: null` when it has none. Management stays the S3 API; this is display.
 */
export function getCors(bucket: string): Promise<{ cors: CorsInfo[] | null }> {
  return http.get<{ cors: CorsInfo[] | null }>(
    `/_/api/buckets/${encodeURIComponent(bucket)}/cors`,
  );
}
