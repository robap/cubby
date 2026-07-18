// Rendering tests for the bucket browser + object-detail sub-view. Pure-render
// assertions seed the browse store's signals directly; interaction tests stub
// `fetch` (the zero/http client calls it with a Request carrying `.url` /
// `.method`) so the store's mutators drive real API calls against canned JSON.

import {
  describe, it, expect, beforeEach, afterEach,
  render, find, findAll, text, fire, cleanup,
} from "zero/test";
import type { BucketInfo, FolderView, ObjectMeta, SearchResult } from "../lib/api.ts";
import Browser from "./browser.ts";
import {
  allBuckets, buckets, folder, objectMeta, prefix,
  presignedUrl, searchResults, searchTerm, selectedBucket, selectedObject, setSearch,
} from "../stores/browse.ts";
import { panelOpen as corsPanelOpen, rules as corsRules } from "../stores/cors.ts";
import { panelOpen as notifPanelOpen } from "../stores/notifications.ts";

/** Reset the module-level store between tests. */
function resetStore(): void {
  buckets.set([]);
  selectedBucket.set(null);
  prefix.set("");
  folder.set(null);
  searchTerm.set("");
  allBuckets.set(false);
  searchResults.set(null);
  selectedObject.set(null);
  objectMeta.set(null);
  presignedUrl.set(null);
  corsPanelOpen.set(false);
  corsRules.set(null);
  notifPanelOpen.set(false);
}

type StubRoute = (url: string, method: string) => unknown;
const g = globalThis as unknown as { fetch: unknown };
const realFetch = g.fetch;

/** Route `fetch` (Request or (url, init)) to canned JSON. */
function stubFetch(route: StubRoute): { calls: { url: string; method: string }[] } {
  const calls: { url: string; method: string }[] = [];
  g.fetch = async (input: unknown, init?: { method?: string }) => {
    const req = input as { url?: string; method?: string };
    const url = typeof input === "string" ? input : req.url ?? "";
    const method = (typeof input === "string" ? init?.method : req.method) ?? "GET";
    calls.push({ url, method });
    const data = route(url, method);
    return {
      ok: true,
      status: 200,
      headers: { get: () => "application/json" },
      json: async () => data,
      text: async () => (typeof data === "string" ? data : JSON.stringify(data)),
    };
  };
  return { calls };
}

/** Drain several micro/macro-task cycles so an async reload + re-render settle.
 * (Continuations schedule as macrotasks here, so a multi-fetch mutator chain —
 * e.g. create → list → select → load — needs one cycle per hop.) */
async function tick(): Promise<void> {
  for (let i = 0; i < 12; i++) await new Promise((r) => setTimeout(r, 0));
}

const bucket = (over: Partial<BucketInfo> = {}): BucketInfo => ({
  name: "app-assets",
  created_at: "2026-06-28T16:40:00Z",
  object_count: 10,
  size: 4_509_715_660,
  ...over,
});

const folderView = (over: Partial<FolderView> = {}): FolderView => ({
  prefix: "",
  delimiter: "/",
  common_prefixes: ["css/", "js/"],
  objects: [
    { key: "README.md", size: 2048, etag: '"c9f0"', last_modified: "2026-06-28T16:40:00Z" },
  ],
  next_continuation_token: null,
  ...over,
});

// One flat describe: zero/test only applies `beforeEach`/`afterEach` to tests
// in the *same* describe (nested describes do not inherit), and every test
// shares the module-level browse store — so the reset must live here.
describe("Browser", () => {
  beforeEach(resetStore);
  afterEach(() => {
    g.fetch = realFetch;
    cleanup();
  });

  it("lists every bucket with its object count and size", () => {
    buckets.set([bucket(), bucket({ name: "logs-archive", object_count: 0, size: 0 })]);
    selectedBucket.set("app-assets");
    const el = render(Browser());
    const rows = findAll(el, ".bucket-row");
    expect(rows.length).toBe(2);
    expect(text(rows[0]!)).toContain("app-assets");
    expect(text(rows[0]!)).toContain("10 objects");
    // Zero-object bucket shows a dash for size, not "0 B".
    expect(text(rows[1]!)).toContain("logs-archive");
    expect(text(rows[1]!)).toContain("0 objects · —");
  });

  it("marks the selected bucket active", () => {
    buckets.set([bucket()]);
    selectedBucket.set("app-assets");
    const el = render(Browser());
    expect(find(el, ".bucket-row.active")).not.toBe(null);
  });

  it("renders folders (trailing slash) then objects", () => {
    selectedBucket.set("app-assets");
    folder.set(folderView());
    const el = render(Browser());
    const folders = findAll(el, ".folder-row");
    expect(folders.length).toBe(2);
    expect(text(folders[0]!)).toContain("css/");
    const objects = findAll(el, ".object-row");
    expect(objects.length).toBe(1);
    expect(text(objects[0]!)).toContain("README.md");
    expect(text(objects[0]!)).toContain("2.0 KB");
  });

  it("shows the empty state with a drop hint for an empty bucket", () => {
    selectedBucket.set("logs-archive");
    folder.set(folderView({ common_prefixes: [], objects: [] }));
    const el = render(Browser());
    expect(text(el, ".empty-state")).toContain("No objects yet");
    expect(text(el, ".empty-state")).toContain("logs-archive/");
  });

  it("builds a breadcrumb from the current prefix", () => {
    selectedBucket.set("app-assets");
    prefix.set("images/hero/");
    folder.set(folderView({ prefix: "images/hero/", common_prefixes: [], objects: [] }));
    const el = render(Browser());
    const crumbs = findAll(el, ".crumb");
    expect(crumbs.map((c) => text(c))).toEqual(["app-assets", "images", "hero"]);
  });

  it("shows a flat match list with the term highlighted and a count", () => {
    selectedBucket.set("app-assets");
    searchTerm.set("png");
    const res: SearchResult = {
      q: "png",
      bucket: "app-assets",
      truncated: false,
      results: [
        { bucket: "app-assets", key: "images/hero/landing-2x.png", size: 2_517_066, etag: '"a"', last_modified: "2026-07-09T14:22:00Z" },
        { bucket: "app-assets", key: "images/hero/landing-1x.png", size: 984_221, etag: '"b"', last_modified: "2026-07-09T14:22:00Z" },
      ],
    };
    searchResults.set(res);
    const el = render(Browser());
    expect(text(el, ".listing-toolbar")).toContain("2 matches");
    const rows = findAll(el, ".search-row");
    expect(rows.length).toBe(2);
    // The substring "png" is wrapped in <mark>.
    expect(text(find(el, ".search-row mark")!)).toBe("png");
  });

  it("tags each hit with its bucket when the scope is all-buckets", () => {
    selectedBucket.set("demo");
    searchTerm.set("report");
    allBuckets.set(true);
    searchResults.set({
      q: "report",
      bucket: null,
      truncated: false,
      results: [
        { bucket: "demo", key: "report.txt", size: 10, etag: '"a"', last_modified: "2026-07-09T14:22:00Z" },
        { bucket: "other", key: "report.csv", size: 20, etag: '"b"', last_modified: "2026-07-09T14:22:00Z" },
      ],
    });
    const el = render(Browser());
    const tags = findAll(el, ".bucket-tag").map((t) => text(t));
    expect(tags).toEqual(["demo", "other"]);
  });

  it("shows a This-bucket/All-buckets scope toggle and flips scope on click", () => {
    selectedBucket.set("demo");
    folder.set(folderView());
    const el = render(Browser());
    const segs = findAll(el, ".seg-btn");
    expect(segs.map((b) => text(b))).toEqual(["This bucket", "All buckets"]);
    // Default scope is this-bucket; the active segment reflects the store.
    expect(text(find(el, ".seg-btn.active")!)).toBe("This bucket");
    fire(segs[1]!, "click"); // → All buckets
    expect(allBuckets.val).toBe(true);
    expect(text(find(el, ".seg-btn.active")!)).toBe("All buckets");
    fire(segs[0]!, "click"); // → back to This bucket
    expect(allBuckets.val).toBe(false);
  });

  it("drills into a folder on click and reloads the listing", async () => {
    selectedBucket.set("app-assets");
    folder.set(folderView({ common_prefixes: ["css/"], objects: [] }));
    stubFetch(() => folderView({ prefix: "css/", common_prefixes: [], objects: [
      { key: "css/app.css", size: 100, etag: '"x"', last_modified: "2026-06-28T16:40:00Z" },
    ] }));
    const el = render(Browser());
    fire(find(el, ".folder-row")!, "click");
    await tick();
    expect(prefix.val).toBe("css/");
    expect(text(el, ".object-row")).toContain("app.css");
  });

  it("deletes an object via the row action", async () => {
    selectedBucket.set("demo");
    folder.set(folderView({ objects: [
      { key: "x.bin", size: 5, etag: '"a"', last_modified: "2026-06-28T16:40:00Z" },
    ], common_prefixes: [] }));
    const stub = stubFetch((url) =>
      url.includes("/objects/x.bin") ? {} : url.includes("/buckets") && !url.includes("/objects")
        ? { buckets: [] } : folderView({ objects: [], common_prefixes: [] }));
    const el = render(Browser());
    fire(find(el, ".row-delete")!, "click");
    await tick();
    const del = stub.calls.find((c) => c.method === "DELETE");
    expect(del?.url).toContain("/_/api/buckets/demo/objects/x.bin");
  });

  const meta = (over: Partial<ObjectMeta> = {}): ObjectMeta => ({
    key: "images/hero/landing-1x.png",
    size: 984_221,
    etag: '"eccbc87e4b5ce2fe"',
    content_type: "image/png",
    last_modified: "2026-07-09T14:22:00Z",
    storage_class: "STANDARD",
    metadata: { "uploaded-by": "build-bot@ci", source: "figma-export" },
    ...over,
  });

  it("wraps the view in a single stable root element across the branch flip", () => {
    // The BrowseView↔ObjectDetail choice must live inside one stable
    // `.browser-root`, not at the component root: zero's router rebuilds this
    // component on every `/_/browser` navigation and swaps it into the layout
    // outlet, and a bare root binding desyncs with an in-app `selectedObject`
    // flip — orphaning a duplicate screen. The wrapper is what the outlet owns.
    selectedBucket.set("app-assets");
    folder.set(folderView());
    const el = render(Browser());
    expect(findAll(el, ".browser-root").length).toBe(1);
    expect(findAll(el, ".browser-screen").length).toBe(1);
    expect(findAll(el, ".detail-screen").length).toBe(0);
    // Flip to the detail branch in place: still exactly one root, one screen.
    selectedObject.set("images/hero/landing-1x.png");
    objectMeta.set(meta());
    expect(findAll(el, ".browser-root").length).toBe(1);
    expect(findAll(el, ".detail-screen").length).toBe(1);
    expect(findAll(el, ".browser-screen").length).toBe(0);
  });

  it("renders the metadata tables and an image preview", () => {
    selectedBucket.set("app-assets");
    selectedObject.set("images/hero/landing-1x.png");
    objectMeta.set(meta());
    const el = render(Browser());
    expect(text(el, ".detail-screen")).toContain("STANDARD");
    expect(text(el, ".detail-screen")).toContain("984,221 bytes");
    expect(text(el, ".detail-screen")).toContain("image/png");
    // User metadata rows render as x-amz-meta-*.
    expect(text(el, ".detail-screen")).toContain("x-amz-meta-uploaded-by");
    // Image type → an <img> preview, not a text pane.
    expect(find(el, ".preview-img")).not.toBe(null);
  });

  it("pretty-prints a minified JSON object in the preview pane", async () => {
    selectedBucket.set("demo");
    selectedObject.set("data.json");
    objectMeta.set(meta({ key: "data.json", content_type: "application/json", size: 20 }));
    // The preview fetch streams the raw (minified) bytes back via `text()`.
    stubFetch(() => '{"a":1,"b":2}');
    const el = render(Browser());
    await tick();
    const pre = find(el, ".preview-text");
    expect(pre).not.toBe(null);
    // Re-indented on its own lines, not one long run.
    expect(text(pre!)).toContain('"a": 1');
    expect(text(pre!).split("\n").length).toBeGreaterThan(1);
  });

  it("mints a presigned URL on Generate and shows it", async () => {
    selectedBucket.set("app-assets");
    selectedObject.set("k.png");
    objectMeta.set(meta({ key: "k.png" }));
    stubFetch(() => ({ url: "http://127.0.0.1:9000/app-assets/k.png?X-Amz-Signature=abc", expires_at: "2026-07-09T15:22:00Z" }));
    const el = render(Browser());
    const generate = findAll(el, "button").find((b) => text(b).includes("Generate"))!;
    fire(generate, "click");
    await tick();
    // The mint lands in the store and is set on the field via the `.value`
    // property (through a ref), which the test DOM reflects.
    expect(presignedUrl.val).toContain("X-Amz-Signature=abc");
    const field = find(el, ".presign-url") as unknown as { value: string } | null;
    expect(field).not.toBe(null);
    expect(field!.value).toContain("X-Amz-Signature=abc");
  });

  it("creates a bucket from the buckets column and selects it", async () => {
    const stub = stubFetch((url, method) => {
      if (method === "POST") return { name: "newb" };
      if (url.includes("/objects")) {
        return { prefix: "", delimiter: "/", common_prefixes: [], objects: [], next_continuation_token: null };
      }
      if (url.includes("/_/api/health")) {
        return { status: "ok", version: "0", uptime_s: 0, data_dir: "d", endpoint: "http://x", region: "us-east-1", bucket_count: 1, object_count: 0 };
      }
      return { buckets: [{ name: "newb", created_at: "2026-07-11T00:00:00Z", object_count: 0, size: 0 }] };
    });
    const el = render(Browser());
    // The `+` toggle lives in the BUCKETS header; it reveals the inline form.
    fire(find(el, ".new-bucket-add")!, "click");
    const input = find(el, ".new-bucket-form input") as HTMLInputElement;
    input.value = "newb";
    fire(input, "input");
    fire(findAll(el, "button").find((b) => text(b) === "Create")!, "click");
    await tick();
    expect(stub.calls.some((c) => c.method === "POST" && c.url.includes("/_/api/buckets"))).toBe(true);
    expect(selectedBucket.val).toBe("newb");
  });

  it("dismisses the new-bucket form on Escape", () => {
    buckets.set([bucket()]);
    const el = render(Browser());
    fire(find(el, ".new-bucket-add")!, "click");
    expect(find(el, ".new-bucket-form")).not.toBe(null);
    // Escape anywhere in the form closes it without creating a bucket.
    fire(find(el, ".new-bucket-form input")!, "keydown", { key: "Escape" });
    expect(find(el, ".new-bucket-form")).toBe(null);
  });

  it("keeps the search input's DOM node across a search (no focus loss)", async () => {
    selectedBucket.set("demo");
    folder.set(folderView({ common_prefixes: ["a/"], objects: [] }));
    stubFetch(() => ({
      q: "s",
      bucket: "demo",
      truncated: false,
      results: [{ bucket: "demo", key: "s.txt", size: 1, etag: '"a"', last_modified: "2026-07-11T00:00:00Z" }],
    }));
    const el = render(Browser());
    const before = find(el, ".search-field input");
    expect(before).not.toBe(null);
    // A keystroke drives a search; the store term/results change and the view
    // swaps folder→search — but the search field must be the SAME node, or the
    // browser would drop focus mid-word.
    await setSearch("s");
    await tick();
    expect(find(el, ".search-results")).not.toBe(null);
    expect(find(el, ".search-field input")).toBe(before);
  });

  it("the CORS toggle opens the read-only CORS panel for the selected bucket", async () => {
    buckets.set([bucket({ name: "uploads" })]);
    selectedBucket.set("uploads");
    folder.set(folderView());
    stubFetch(() => ({
      cors: [{
        AllowedOrigins: ["http://localhost:3000"],
        AllowedMethods: ["GET", "PUT"],
        ExposeHeaders: ["ETag"],
        MaxAgeSeconds: 600,
      }],
    }));

    const el = render(Browser());
    fire(find(el, ".cors-toggle")!, "click");
    await tick();

    expect(find(el, ".cors-panel")).not.toBe(null);
    expect(text(el, ".cors-list")).toContain("http://localhost:3000");
    expect(text(el, ".cors-list")).toContain("ETag");
  });

  it("opening CORS closes the notifications panel (they share the pane)", async () => {
    buckets.set([bucket({ name: "uploads" })]);
    selectedBucket.set("uploads");
    folder.set(folderView());
    stubFetch((url) =>
      url.includes("/cors") ? { cors: null } : { notifications: [] });

    const el = render(Browser());
    // Open notifications first.
    fire(find(el, ".notifications-toggle")!, "click");
    await tick();
    expect(find(el, ".notifications-panel")).not.toBe(null);
    // Now open CORS — notifications must close, CORS must show.
    fire(find(el, ".cors-toggle")!, "click");
    await tick();
    expect(find(el, ".notifications-panel")).toBe(null);
    expect(find(el, ".cors-panel")).not.toBe(null);
  });
});
