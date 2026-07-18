// Store tests for the read-only CORS panel: open/close and the load mutator
// driving the read-only seam (fetch stubbed to canned JSON). There is no
// add/remove — management is the S3 API, not this seam.

import { describe, it, expect, beforeEach, afterEach } from "zero/test";
import type { CorsInfo } from "../lib/api.ts";
import { closePanel, load, loadedBucket, openPanel, panelOpen, rules } from "./cors.ts";

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
      ok: true,
      status: 200,
      headers: { get: () => "application/json" },
      json: async () => data,
      text: async () => (typeof data === "string" ? data : JSON.stringify(data)),
    };
  };
  return { calls };
}

const rule = (over: Partial<CorsInfo> = {}): CorsInfo => ({
  AllowedOrigins: ["http://localhost:3000"],
  AllowedMethods: ["GET", "PUT"],
  AllowedHeaders: ["*"],
  ExposeHeaders: ["ETag"],
  MaxAgeSeconds: 600,
  ...over,
});

describe("cors store", () => {
  beforeEach(() => {
    panelOpen.set(false);
    rules.set(null);
    loadedBucket.set(null);
  });
  afterEach(() => {
    g.fetch = realFetch;
  });

  it("openPanel loads the bucket's rules and opens", async () => {
    const { calls } = stubFetch(() => ({ cors: [rule()] }));
    await openPanel("uploads");
    expect(panelOpen.val).toBe(true);
    expect(loadedBucket.val).toBe("uploads");
    expect(rules.val?.length).toBe(1);
    // It GETs the read-only cors seam for the bucket.
    expect(calls[0].url).toContain("/buckets/uploads/cors");
    expect(calls[0].method).toBe("GET");
  });

  it("load sets rules to null when the bucket has no config", async () => {
    stubFetch(() => ({ cors: null }));
    await load("plain");
    expect(loadedBucket.val).toBe("plain");
    expect(rules.val).toBe(null);
  });

  it("closePanel just closes", () => {
    panelOpen.set(true);
    closePanel();
    expect(panelOpen.val).toBe(false);
  });
});
