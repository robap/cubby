// Store tests for the Notifications panel: open/close and the load/add/remove
// mutators driving the seam (fetch stubbed to canned JSON).

import { describe, it, expect, beforeEach, afterEach } from "zero/test";
import type { NotificationInfo } from "../lib/api.ts";
import {
  add, closePanel, load, loadedBucket, notifications, openPanel, panelOpen, remove,
} from "./notifications.ts";

type StubRoute = (url: string, method: string) => unknown;
const g = globalThis as unknown as { fetch: unknown };
const realFetch = g.fetch;

function stubFetch(route: StubRoute): { calls: { url: string; method: string }[] } {
  const calls: { url: string; method: string }[] = [];
  g.fetch = async (input: unknown, init?: { method?: string }) => {
    const req = input as { url?: string; method?: string };
    const url = typeof input === "string" ? input : req.url ?? "";
    const method = (typeof input === "string" ? init?.method : req.method) ?? "GET";
    calls.push({ url, method });
    const data = route(url, method);
    return {
      ok: true, status: 200,
      headers: { get: () => "application/json" },
      json: async () => data,
      text: async () => (typeof data === "string" ? data : JSON.stringify(data)),
    };
  };
  return { calls };
}

const dest = (over: Partial<NotificationInfo> = {}): NotificationInfo => ({
  id: 1, url: "http://a/hook", events: ["s3:ObjectCreated:*"],
  prefix: null, suffix: null, format: "s3-notification",
  timeout_ms: 5000, created_at: "2026-07-17T00:00:00Z", ...over,
});

describe("notifications store", () => {
  beforeEach(() => {
    panelOpen.set(false);
    notifications.set([]);
    loadedBucket.set(null);
  });
  afterEach(() => { g.fetch = realFetch; });

  it("openPanel loads the bucket's destinations and opens", async () => {
    stubFetch(() => ({ notifications: [dest({ id: 3 })] }));
    await openPanel("uploads");
    expect(panelOpen.val).toBe(true);
    expect(loadedBucket.val).toBe("uploads");
    expect(notifications.val.map((n) => n.id)).toEqual([3]);
  });

  it("closePanel just closes", () => {
    panelOpen.set(true);
    closePanel();
    expect(panelOpen.val).toBe(false);
  });

  it("add POSTs then refreshes the list", async () => {
    const created = dest({ id: 9, url: "http://new/hook" });
    const { calls } = stubFetch((_url, method) =>
      method === "POST" ? created : { notifications: [created] });
    await add("uploads", { url: "http://new/hook", events: ["s3:ObjectCreated:*"] });
    expect(calls.some((c) => c.method === "POST")).toBe(true);
    expect(notifications.val.map((n) => n.id)).toEqual([9]);
  });

  it("remove DELETEs then refreshes the list", async () => {
    notifications.set([dest({ id: 4 })]);
    const { calls } = stubFetch((_url, method) =>
      method === "DELETE" ? "" : { notifications: [] });
    await remove("uploads", 4);
    const del = calls.find((c) => c.method === "DELETE");
    expect(del?.url).toContain("/notifications/4");
    expect(notifications.val.length).toBe(0);
  });

  it("load records the bucket it loaded", async () => {
    stubFetch(() => ({ notifications: [] }));
    await load("photos");
    expect(loadedBucket.val).toBe("photos");
  });
});
