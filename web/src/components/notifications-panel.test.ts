// Rendering tests for the per-bucket Notifications panel. Pure-render assertions
// seed the notifications store directly; interaction tests stub `fetch` (the
// zero/http client calls it with a Request carrying `.url`/`.method`) so the
// store's mutators drive real API calls against canned JSON.

import {
  describe, it, expect, beforeEach, afterEach,
  render, find, findAll, text, fire, cleanup,
} from "zero/test";
import type { NotificationInfo } from "../lib/api.ts";
import NotificationsPanel from "./notifications-panel.ts";
import { loadedBucket, notifications, panelOpen } from "../stores/notifications.ts";
import { selectedBucket } from "../stores/browse.ts";

function resetStore(): void {
  panelOpen.set(false);
  notifications.set([]);
  loadedBucket.set(null);
  selectedBucket.set("uploads");
}

type StubRoute = (url: string, method: string, body: string | null) => unknown;
const g = globalThis as unknown as { fetch: unknown };
const realFetch = g.fetch;

/** Route `fetch` (Request or (url, init)) to canned JSON, recording calls. */
function stubFetch(route: StubRoute): { calls: { url: string; method: string; body: string | null }[] } {
  const calls: { url: string; method: string; body: string | null }[] = [];
  g.fetch = async (input: unknown, init?: { method?: string; body?: string }) => {
    const req = input as { url?: string; method?: string; text?: () => Promise<string> };
    const url = typeof input === "string" ? input : req.url ?? "";
    const method = (typeof input === "string" ? init?.method : req.method) ?? "GET";
    // The body may arrive via `init.body` or inside a Request (zero/http sends a
    // Request for JSON writes) — read whichever is present.
    let body: string | null = init?.body ?? null;
    if (body === null && typeof input !== "string" && typeof req.text === "function") {
      try { body = await req.text(); } catch { body = null; }
    }
    calls.push({ url, method, body });
    const data = route(url, method, body);
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

/** Drain several task cycles so an async mutator chain settles. */
async function tick(): Promise<void> {
  for (let i = 0; i < 12; i++) await new Promise((r) => setTimeout(r, 0));
}

const dest = (over: Partial<NotificationInfo> = {}): NotificationInfo => ({
  id: 1,
  url: "http://localhost:3000/hook",
  events: ["s3:ObjectCreated:*"],
  prefix: "photos/",
  suffix: ".jpg",
  format: "s3-notification",
  timeout_ms: 5000,
  created_at: "2026-07-17T18:22:05Z",
  ...over,
});

describe("NotificationsPanel", () => {
  beforeEach(resetStore);
  afterEach(() => {
    g.fetch = realFetch;
    cleanup();
  });

  it("lists each destination's url and events", () => {
    notifications.set([
      dest({ id: 1, url: "http://a/hook", events: ["s3:ObjectCreated:*"] }),
      dest({ id: 2, url: "http://b/hook", events: ["s3:ObjectRemoved:*"] }),
    ]);
    const el = render(NotificationsPanel());
    const rows = findAll(el, ".notification-row");
    expect(rows.length).toBe(2);
    expect(text(el, ".notification-row")).toContain("http://a/hook");
  });

  it("renders a row's events, prefix, suffix, format, and timeout", () => {
    notifications.set([
      dest({
        events: ["s3:ObjectCreated:*", "s3:ObjectRemoved:*"],
        prefix: "photos/",
        suffix: ".jpg",
        format: "eventbridge",
        timeout_ms: 1234,
      }),
    ]);
    const el = render(NotificationsPanel());
    const meta = text(el, ".notification-meta");
    expect(meta).toContain("s3:ObjectCreated:*, s3:ObjectRemoved:*");
    expect(meta).toContain("prefix: photos/");
    expect(meta).toContain("suffix: .jpg");
    expect(meta).toContain("eventbridge");
    expect(meta).toContain("1234ms");
  });

  it("omits absent prefix/suffix from the row meta", () => {
    notifications.set([dest({ prefix: null, suffix: null })]);
    const el = render(NotificationsPanel());
    const meta = text(el, ".notification-meta");
    expect(meta).not.toContain("prefix:");
    expect(meta).not.toContain("suffix:");
  });

  it("offers every subscribable event as a checkbox", () => {
    const el = render(NotificationsPanel());
    for (const ev of [
      "s3:ObjectCreated:*",
      "s3:ObjectCreated:Put",
      "s3:ObjectCreated:Copy",
      "s3:ObjectCreated:CompleteMultipartUpload",
      "s3:ObjectRemoved:*",
      "s3:ObjectRemoved:Delete",
    ]) {
      expect(find(el, `[data-event='${ev}'] input`)).not.toBe(null);
    }
  });

  it("shows an empty state when there are no destinations", () => {
    notifications.set([]);
    const el = render(NotificationsPanel());
    expect(findAll(el, ".notification-row").length).toBe(0);
    expect(text(el, ".notifications-empty")).toContain("No");
  });

  it("adds a destination via the form, posting to the seam", async () => {
    const created = dest({ id: 7, url: "http://new/hook", prefix: null, suffix: null });
    const { calls } = stubFetch((_url, method) => {
      if (method === "POST") return created;
      // GET refresh returns the new list.
      return { notifications: [created] };
    });

    const el = render(NotificationsPanel());
    // Fill the url and check a created-event box.
    const url = find(el, ".notification-url-input input") as HTMLInputElement;
    url.value = "http://new/hook";
    fire(url, "input");
    const createdBox = find(el, "[data-event='s3:ObjectCreated:*'] input") as HTMLInputElement;
    fire(createdBox, "click");
    // Submit.
    fire(find(el, ".notification-add-form")!, "submit");
    await tick();

    const posts = calls.filter((c) => c.method === "POST");
    expect(posts.length).toBe(1);
    expect(posts[0].url).toContain("/buckets/uploads/notifications");
    expect(posts[0].body).toContain("http://new/hook");
    expect(posts[0].body).toContain("s3:ObjectCreated:*");
    // The list refreshed to include the new destination.
    expect(text(el, ".notifications-list")).toContain("http://new/hook");
  });

  it("adds a second destination when one already exists (no error, both listed)", async () => {
    // Start with one existing destination — the add path must reconcile a
    // non-empty list, not just render the empty→one transition.
    const existing = dest({ id: 1, url: "http://a/hook" });
    const created = dest({ id: 2, url: "http://b/hook", prefix: "docs/", suffix: null });
    notifications.set([existing]);
    stubFetch((_url, method) =>
      method === "POST" ? created : { notifications: [existing, created] });

    const el = render(NotificationsPanel());
    const url = find(el, ".notification-url-input input") as HTMLInputElement;
    url.value = "http://b/hook";
    fire(url, "input");
    fire(find(el, "[data-event='s3:ObjectCreated:*'] input")!, "click");
    fire(find(el, ".notification-add-form")!, "submit");
    await tick();

    // Both destinations render, and no error alert appeared.
    expect(findAll(el, ".notification-row").length).toBe(2);
    const list = text(el, ".notifications-list");
    expect(list).toContain("http://a/hook");
    expect(list).toContain("http://b/hook");
    expect(find(el, ".notification-error")).toBe(null);
  });

  it("clears the form after a successful add", async () => {
    const created = dest({ id: 3, url: "http://new/hook", prefix: null, suffix: null });
    stubFetch((_url, method) => (method === "POST" ? created : { notifications: [created] }));

    const el = render(NotificationsPanel());
    const urlInput = find(el, ".notification-url-input input") as HTMLInputElement;
    urlInput.value = "http://new/hook";
    fire(urlInput, "input");
    const prefixInput = find(el, ".notification-prefix-input input") as HTMLInputElement;
    prefixInput.value = "photos/";
    fire(prefixInput, "input");
    const box = find(el, "[data-event='s3:ObjectCreated:*'] input") as HTMLInputElement;
    fire(box, "click");

    fire(find(el, ".notification-add-form")!, "submit");
    await tick();

    // Text fields cleared…
    expect((find(el, ".notification-url-input input") as HTMLInputElement).value).toBe("");
    expect((find(el, ".notification-prefix-input input") as HTMLInputElement).value).toBe("");
    // …and the event selection cleared (checkbox unchecked).
    expect((find(el, "[data-event='s3:ObjectCreated:*'] input") as HTMLInputElement).checked).toBe(
      false,
    );
  });

  it("select-all checks every event; a second click clears them", async () => {
    const created = dest({ id: 4, url: "http://all/hook" });
    const { calls } = stubFetch((_url, method) =>
      method === "POST" ? created : { notifications: [created] });

    const el = render(NotificationsPanel());
    const urlInput = find(el, ".notification-url-input input") as HTMLInputElement;
    urlInput.value = "http://all/hook";
    fire(urlInput, "input");

    // Select all → the POST carries every event.
    fire(find(el, "[data-event='__all__'] input")!, "click");
    fire(find(el, ".notification-add-form")!, "submit");
    await tick();
    let body = JSON.parse(calls.find((c) => c.method === "POST")!.body!);
    expect(body.events.length).toBe(6);
    expect(body.events).toContain("s3:ObjectCreated:*");
    expect(body.events).toContain("s3:ObjectRemoved:Delete");
  });

  it("select-all then unselect-all sends no events", async () => {
    const created = dest({ id: 5, url: "http://none/hook" });
    const { calls } = stubFetch((_url, method) =>
      method === "POST" ? created : { notifications: [created] });

    const el = render(NotificationsPanel());
    const urlInput = find(el, ".notification-url-input input") as HTMLInputElement;
    urlInput.value = "http://none/hook";
    fire(urlInput, "input");

    const all = find(el, "[data-event='__all__'] input")!;
    fire(all, "click"); // select all
    fire(all, "click"); // clear all
    fire(find(el, ".notification-add-form")!, "submit");
    await tick();
    const body = JSON.parse(calls.find((c) => c.method === "POST")!.body!);
    expect(body.events).toEqual([]);
  });

  it("toggles an event off and omits empty filters in the POST body", async () => {
    const created = dest({ id: 8, url: "http://new/hook", prefix: "photos/", suffix: null });
    const { calls } = stubFetch((_url, method) =>
      method === "POST" ? created : { notifications: [created] });

    const el = render(NotificationsPanel());
    const url = find(el, ".notification-url-input input") as HTMLInputElement;
    url.value = "http://new/hook";
    fire(url, "input");
    // Select then deselect ObjectCreated:*, then select ObjectCreated:Put.
    const wildcard = find(el, "[data-event='s3:ObjectCreated:*'] input")!;
    fire(wildcard, "click");
    fire(wildcard, "click");
    fire(find(el, "[data-event='s3:ObjectCreated:Put'] input")!, "click");
    // Set a prefix; leave suffix empty.
    const prefix = find(el, ".notification-prefix-input input") as HTMLInputElement;
    prefix.value = "photos/";
    fire(prefix, "input");

    fire(find(el, ".notification-add-form")!, "submit");
    await tick();

    const post = calls.find((c) => c.method === "POST")!;
    const body = JSON.parse(post.body!);
    // The wildcard was toggled back off — only Put remains.
    expect(body.events).toEqual(["s3:ObjectCreated:Put"]);
    // Set prefix is sent; empty suffix is omitted (not sent as "").
    expect(body.prefix).toBe("photos/");
    expect("suffix" in body).toBe(false);
  });

  it("surfaces a seam validation error under the form", async () => {
    const envelope = { error: { code: "InvalidNotification", message: "bad destination" } };
    g.fetch = async () => ({
      ok: false,
      status: 400,
      headers: { get: () => "application/json" },
      json: async () => envelope,
      text: async () => JSON.stringify(envelope),
    });
    const el = render(NotificationsPanel());
    const url = find(el, ".notification-url-input input") as HTMLInputElement;
    url.value = "https://x/hook";
    fire(url, "input");
    fire(find(el, "[data-event='s3:ObjectCreated:*'] input")!, "click");
    fire(find(el, ".notification-add-form")!, "submit");
    await tick();
    // The error span appears (it is hidden while error is null).
    expect(find(el, ".notification-error")).not.toBe(null);
  });

  it("deletes a destination via its row action", async () => {
    notifications.set([dest({ id: 5, url: "http://gone/hook" })]);
    const { calls } = stubFetch((_url, method) => {
      if (method === "DELETE") return "";
      return { notifications: [] }; // refresh after delete
    });

    const el = render(NotificationsPanel());
    fire(find(el, ".notification-row .row-delete")!, "click");
    await tick();

    const dels = calls.filter((c) => c.method === "DELETE");
    expect(dels.length).toBe(1);
    expect(dels[0].url).toContain("/notifications/5");
    expect(findAll(el, ".notification-row").length).toBe(0);
  });
});
