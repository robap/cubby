// Store-level tests for the URL→state hydration (`applyLocation`), the single
// loader behind cold load, deep links, and Back/Forward. Fetch is stubbed the
// way the browser render tests stub it; `navigate` is a no-op here (no live
// router), so these assert the store signals and the fetches they drive.

import { describe, it, expect, beforeEach, afterEach } from "zero/test";
import type { FolderView, ObjectMeta } from "../lib/api.ts";
import {
  applyLocation,
  buckets,
  folder,
  objectMeta,
  prefix,
  presignedUrl,
  searchResults,
  searchTerm,
  selectedBucket,
  selectedObject,
} from "./browse.ts";

/** Reset the module-level store between tests. */
function resetStore(): void {
  buckets.set([]);
  selectedBucket.set(null);
  prefix.set("");
  folder.set(null);
  searchTerm.set("");
  searchResults.set(null);
  selectedObject.set(null);
  objectMeta.set(null);
  presignedUrl.set(null);
}

const g = globalThis as unknown as { fetch: unknown };
const realFetch = g.fetch;

/** Route `fetch` to canned JSON, recording every request URL. */
function stubFetch(route: (url: string) => unknown): { urls: string[] } {
  const urls: string[] = [];
  g.fetch = async (input: unknown) => {
    const url = typeof input === "string" ? input : (input as { url?: string }).url ?? "";
    urls.push(url);
    const data = route(url);
    return {
      ok: true,
      status: 200,
      headers: { get: () => "application/json" },
      json: async () => data,
      text: async () => (typeof data === "string" ? data : JSON.stringify(data)),
    };
  };
  return { urls };
}

const folderView = (over: Partial<FolderView> = {}): FolderView => ({
  prefix: "",
  delimiter: "/",
  common_prefixes: [],
  objects: [{ key: "a.txt", size: 1, etag: '"a"', last_modified: "2026-07-11T00:00:00Z" }],
  next_continuation_token: null,
  ...over,
});

const meta = (over: Partial<ObjectMeta> = {}): ObjectMeta => ({
  key: "docs/report.pdf",
  size: 10,
  etag: '"e"',
  content_type: "application/pdf",
  last_modified: "2026-07-11T00:00:00Z",
  storage_class: "STANDARD",
  metadata: {},
  ...over,
});

describe("applyLocation", () => {
  beforeEach(resetStore);
  afterEach(() => {
    g.fetch = realFetch;
  });

  it("hydrates a folder location: sets bucket + prefix and loads the folder", async () => {
    stubFetch(() => folderView({ prefix: "a/" }));
    await applyLocation({ bucket: "demo", prefix: "a/", object: null });
    expect(selectedBucket.val).toBe("demo");
    expect(prefix.val).toBe("a/");
    expect(folder.val?.prefix).toBe("a/");
    expect(selectedObject.val).toBe(null);
  });

  it("hydrates an object location: loads its folder and its metadata", async () => {
    stubFetch((url) => (url.includes("/objects/") ? meta() : folderView({ prefix: "docs/" })));
    await applyLocation({ bucket: "demo", prefix: "docs/", object: "docs/report.pdf" });
    expect(selectedBucket.val).toBe("demo");
    expect(prefix.val).toBe("docs/");
    expect(selectedObject.val).toBe("docs/report.pdf");
    expect(objectMeta.val?.key).toBe("docs/report.pdf");
  });

  it("is idempotent: re-applying the same folder location does not refetch", async () => {
    const stub = stubFetch(() => folderView({ prefix: "a/" }));
    await applyLocation({ bucket: "demo", prefix: "a/", object: null });
    const afterFirst = stub.urls.length;
    await applyLocation({ bucket: "demo", prefix: "a/", object: null });
    // Same bucket + prefix and a non-null folder → the diff skips the refetch.
    expect(stub.urls.length).toBe(afterFirst);
  });

  it("reloads the folder when the prefix changes within the same bucket", async () => {
    stubFetch((url) => folderView({ prefix: url.includes("prefix=b") ? "b/" : "a/" }));
    await applyLocation({ bucket: "demo", prefix: "a/", object: null });
    expect(folder.val?.prefix).toBe("a/");
    await applyLocation({ bucket: "demo", prefix: "b/", object: null });
    expect(prefix.val).toBe("b/");
    expect(folder.val?.prefix).toBe("b/");
  });

  it("clears an active search when the bucket changes", async () => {
    stubFetch(() => folderView());
    searchTerm.set("report");
    searchResults.set({ q: "report", bucket: "demo", truncated: false, results: [] });
    await applyLocation({ bucket: "other", prefix: "", object: null });
    expect(searchTerm.val).toBe("");
    expect(searchResults.val).toBe(null);
  });

  it("clears an open object when navigating back to its folder", async () => {
    stubFetch((url) => (url.includes("/objects/") ? meta() : folderView({ prefix: "docs/" })));
    await applyLocation({ bucket: "demo", prefix: "docs/", object: "docs/report.pdf" });
    expect(selectedObject.val).not.toBe(null);
    await applyLocation({ bucket: "demo", prefix: "docs/", object: null });
    expect(selectedObject.val).toBe(null);
    expect(objectMeta.val).toBe(null);
  });

  it("loads the bucket list for the default landing (no bucket)", async () => {
    const stub = stubFetch(() => ({
      buckets: [{ name: "first", created_at: "2026-07-11T00:00:00Z", object_count: 0, size: 0 }],
    }));
    await applyLocation({ bucket: null, prefix: "", object: null });
    expect(buckets.val.map((b) => b.name)).toEqual(["first"]);
    // No object/folder fetch on the bare-landing path (only the bucket list).
    expect(stub.urls.every((u) => u.includes("/buckets"))).toBe(true);
  });
});
