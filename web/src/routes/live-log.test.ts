// Rendering tests for the live-log screen. We stub `EventSource` (absent from
// the test DOM) with a controllable fake, dispatch S3 events, and assert the
// table's own logic: empty state, a frame becomes a row, click-to-expand, the
// filter, and pause buffering.
//
// Note: `cleanup()` disposes the mounted component but the test runtime can
// carry an extra open fake across tests, so we (a) assert subscription by URL
// rather than an exact source count and (b) broadcast emits to every open
// source — only the current component's source is wired to the current `el`,
// so exactly one row lands where we assert. Leaked sources feed unmounted
// signals and are harmless.

import {
  describe, it, expect, beforeEach, afterEach,
  render, find, findAll, text, fire, cleanup,
} from "zero/test";
import type { LogEvent } from "../lib/api.ts";
import LiveLog from "./live-log.ts";

/** A controllable stand-in for the browser `EventSource`. */
type FakeSource = {
  url: string;
  onmessage: ((e: { data: string }) => void) | null;
  closed: boolean;
  close(): void;
};

const sources: FakeSource[] = [];

/** `new EventSource(url)` factory — a constructor returning an object. */
function makeEventSource(url: string): FakeSource {
  const s: FakeSource = {
    url,
    onmessage: null,
    closed: false,
    close() {
      this.closed = true;
    },
  };
  sources.push(s);
  return s;
}

/** The still-open sources. */
function openSources(): FakeSource[] {
  return sources.filter((s) => !s.closed);
}

/** Deliver a default (`onmessage`) frame to every open source (see file note). */
function emitFrame(data: string): void {
  for (const s of openSources()) s.onmessage?.({ data });
}

/** Deliver one event frame to every open source. */
function emit(event: Partial<LogEvent>): void {
  emitFrame(JSON.stringify(event));
}

/** Deliver the server's clear frame (rides the default data channel). */
function emitClear(): void {
  emitFrame('{"clear":true}');
}

const sampleEvent = (over: Partial<LogEvent> = {}): LogEvent => ({
  id: 1,
  ts: 1000,
  method: "PUT",
  op: "PutObject",
  bucket: "demo",
  key: "k.bin",
  status: 200,
  duration_ms: 5,
  bytes_in: 10,
  bytes_out: 0,
  auth: "header",
  error_code: null,
  ...over,
});

/** Let the per-animation-frame batch flush and the rows commit. */
function flushFrames(): Promise<void> {
  return new Promise((res) =>
    requestAnimationFrame(() => requestAnimationFrame(() => res())),
  );
}

const g = globalThis as unknown as { EventSource?: unknown };
g.EventSource = makeEventSource;

describe("LiveLog", () => {
  beforeEach(() => {
    sources.length = 0;
  });
  afterEach(cleanup);

  it("shows the waiting empty state before any traffic", () => {
    const el = render(LiveLog());
    expect(text(el, ".empty-state")).toContain("Waiting for S3 traffic");
    expect(findAll(el, ".log-row").length).toBe(0);
  });

  it("subscribes to the SSE endpoint on mount", () => {
    render(LiveLog());
    expect(openSources().some((s) => s.url === "/_/api/events")).toBe(true);
  });

  it("renders a row when an S3 event arrives", async () => {
    const el = render(LiveLog());
    emit(sampleEvent());
    await flushFrames();
    const rows = findAll(el, ".log-row");
    expect(rows.length).toBe(1);
    expect(text(rows[0]!, ".c-op")).toContain("PutObject");
    expect(text(rows[0]!, ".c-method")).toContain("PUT");
    expect(text(rows[0]!, ".c-status")).toContain("200");
  });

  it("renders newest events first (top of the table)", async () => {
    const el = render(LiveLog());
    emit(sampleEvent({ id: 1, op: "PutObject", key: "old.txt" }));
    emit(sampleEvent({ id: 2, op: "GetObject", key: "new.txt" }));
    await flushFrames();
    const rows = findAll(el, ".log-row");
    expect(rows.length).toBe(2);
    // Newest (id 2) sits at the top; oldest (id 1) below it.
    expect(text(rows[0]!, ".c-key")).toContain("new.txt");
    expect(text(rows[1]!, ".c-key")).toContain("old.txt");
  });

  it("renders a human time-ago label in the TIME cell", async () => {
    const el = render(LiveLog());
    // An event two minutes old reads `2m` (not `0.00s`); the ±ms jitter around
    // 130s stays comfortably inside the 2-minute bucket.
    emit(sampleEvent({ ts: Date.now() - 130_000 }));
    await flushFrames();
    // `.c-time` also matches the header cell (index 0); the row's cell is [1].
    const timeCells = findAll(el, ".c-time");
    expect(text(timeCells[1]!)).toBe("2m");
  });

  it("expands a row's full field set on click", async () => {
    const el = render(LiveLog());
    emit(sampleEvent({ auth: "presigned" }));
    await flushFrames();
    expect(findAll(el, ".detail-grid").length).toBe(0);
    fire(find(el, ".log-row")!, "click");
    expect(findAll(el, ".detail-grid").length).toBe(1);
    expect(text(el, ".detail-grid")).toContain("presigned");
  });

  it("offers a View object deep link on an object row", async () => {
    const el = render(LiveLog());
    emit(sampleEvent({ id: 1, op: "GetObject", bucket: "demo", key: "a/b c.txt" }));
    await flushFrames();
    fire(find(el, ".log-row")!, "click");
    const link = find(el, ".detail-jump") as HTMLAnchorElement | null;
    expect(link).not.toBe(null);
    // Rides on the URL codec: the object's own bucket + key, spaces as %20.
    expect(link!.getAttribute("href")).toBe("/_/browser?bucket=demo&object=a%2Fb%20c.txt");
  });

  it("shows no View object link when the event has no key", async () => {
    const el = render(LiveLog());
    emit(sampleEvent({ id: 2, op: "ListBuckets", bucket: null, key: null }));
    await flushFrames();
    fire(find(el, ".log-row")!, "click");
    expect(find(el, ".detail-jump")).toBe(null);
  });

  it("filters rows by op/key/method as you type", async () => {
    const el = render(LiveLog());
    emit(sampleEvent({ id: 1, op: "PutObject", key: "photo.jpg" }));
    emit(sampleEvent({ id: 2, op: "GetObject", key: "report.pdf" }));
    await flushFrames();
    expect(findAll(el, ".log-row").length).toBe(2);

    const input = (find(el, ".toolbar-filter input") ?? find(el, "input"))! as HTMLInputElement;
    input.value = "report";
    fire(input, "input");
    const rows = findAll(el, ".log-row");
    expect(rows.length).toBe(1);
    expect(text(rows[0]!, ".c-key")).toContain("report.pdf");
  });

  it("Clear empties the table, resets the count, and drains the server ring", async () => {
    const gf = globalThis as unknown as { fetch: unknown };
    const realFetch = gf.fetch;
    const calls: { url: string; method: string }[] = [];
    gf.fetch = async (input: unknown, init?: { method?: string }) => {
      const req = input as { url?: string; method?: string };
      const url = typeof input === "string" ? input : req.url ?? "";
      const method = (typeof input === "string" ? init?.method : req.method) ?? "GET";
      calls.push({ url, method });
      return { ok: true, status: 204, headers: { get: () => null }, json: async () => ({}), text: async () => "" };
    };
    try {
      const el = render(LiveLog());
      emit(sampleEvent({ id: 1 }));
      emit(sampleEvent({ id: 2 }));
      await flushFrames();
      expect(findAll(el, ".log-row").length).toBe(2);

      fire(find(el, ".clear-btn")!, "click");
      await flushFrames();
      expect(findAll(el, ".log-row").length).toBe(0);
      expect(text(el, ".count")).toBe("0 / 0");
      expect(
        calls.some((c) => c.method === "POST" && c.url.includes("/_/api/events/clear")),
      ).toBe(true);
    } finally {
      gf.fetch = realFetch;
    }
  });

  it("empties the table when the server broadcasts a clear frame (other tab cleared)", async () => {
    const el = render(LiveLog());
    emit(sampleEvent({ id: 1 }));
    emit(sampleEvent({ id: 2 }));
    await flushFrames();
    expect(findAll(el, ".log-row").length).toBe(2);

    emitClear();
    await flushFrames();
    expect(findAll(el, ".log-row").length).toBe(0);
    expect(text(el, ".count")).toBe("0 / 0");
  });

  it("pauses live inserts and surfaces an N-new badge", async () => {
    const el = render(LiveLog());
    // Pause via the toolbar button.
    const pauseBtn = find(el, ".pause-btn")!;
    fire(pauseBtn, "click");
    emit(sampleEvent({ id: 1 }));
    emit(sampleEvent({ id: 2 }));
    await flushFrames();
    // Buffered while paused — no rows yet; the button is in its paused (play)
    // state and shows the new count beside the icon.
    expect(findAll(el, ".log-row").length).toBe(0);
    expect(find(el, ".pause-btn.paused")).not.toBe(null);
    expect(text(el, ".pause-btn")).toContain("2");
    // Resume flushes the buffer.
    fire(find(el, ".pause-btn")!, "click");
    await flushFrames();
    expect(findAll(el, ".log-row").length).toBe(2);
  });
});
