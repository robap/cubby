import { describe, it, expect } from "zero/test";
import { appendCapped, elapsedLabel, matchesFilter } from "./log.ts";
import type { LogEvent } from "./api.ts";

/**
 * Build a LogEvent with sensible defaults, overridden per test.
 * @param {Partial<LogEvent>} over
 * @returns {LogEvent}
 */
function ev(over: Partial<LogEvent> = {}): LogEvent {
  return {
    id: 1,
    ts: 0,
    method: "GET",
    op: "GetObject",
    bucket: "demo",
    key: "a/b.txt",
    status: 200,
    duration_ms: 3,
    bytes_in: 0,
    bytes_out: 10,
    auth: "header",
    error_code: null,
    ...over,
  };
}

describe("matchesFilter", () => {
  it("passes everything with empty controls", () => {
    expect(matchesFilter(ev(), "", "all", "any")).toBe(true);
  });

  it("filters by status class", () => {
    expect(matchesFilter(ev({ status: 200 }), "", "2", "any")).toBe(true);
    expect(matchesFilter(ev({ status: 404 }), "", "2", "any")).toBe(false);
    expect(matchesFilter(ev({ status: 403 }), "", "4", "any")).toBe(true);
  });

  it("filters by auth", () => {
    expect(matchesFilter(ev({ auth: "presigned" }), "", "all", "presigned")).toBe(true);
    expect(matchesFilter(ev({ auth: "header" }), "", "all", "presigned")).toBe(false);
  });

  it("substring-matches method, op, and bucket/key (case-insensitive)", () => {
    expect(matchesFilter(ev({ op: "PutObject" }), "put", "all", "any")).toBe(true);
    expect(matchesFilter(ev({ key: "reports/x" }), "REPORT", "all", "any")).toBe(true);
    expect(matchesFilter(ev({ method: "DELETE" }), "del", "all", "any")).toBe(true);
    expect(matchesFilter(ev(), "nomatch", "all", "any")).toBe(false);
  });

  it("tolerates a null op / key", () => {
    expect(matchesFilter(ev({ op: null, key: null }), "", "all", "any")).toBe(true);
    expect(matchesFilter(ev({ op: null, key: null, bucket: null }), "get", "all", "any")).toBe(
      true,
    );
  });

  it("does not match a filter against a null op's fallback", () => {
    // The `op ?? ""` fallback must contribute nothing to match against.
    const e = ev({ op: null, method: "GET", bucket: "b", key: "k" });
    expect(matchesFilter(e, "put", "all", "any")).toBe(false);
    expect(matchesFilter(e, "get", "all", "any")).toBe(true);
    // The fallback is an empty string, not any placeholder text of its own.
    expect(matchesFilter(e, "zero", "all", "any")).toBe(false);
  });
});

describe("appendCapped", () => {
  it("appends in order", () => {
    const out = appendCapped([ev({ id: 1 })], [ev({ id: 2 }), ev({ id: 3 })], 10);
    expect(out.map((e) => e.id)).toEqual([1, 2, 3]);
  });

  it("keeps only the last `max` events", () => {
    const prev = [ev({ id: 1 }), ev({ id: 2 })];
    const out = appendCapped(prev, [ev({ id: 3 }), ev({ id: 4 })], 3);
    expect(out.map((e) => e.id)).toEqual([2, 3, 4]);
  });

  it("returns the same list for an empty batch", () => {
    const prev = [ev({ id: 1 })];
    expect(appendCapped(prev, [], 10)).toBe(prev);
  });
});

describe("elapsedLabel", () => {
  it("formats seconds since origin to two decimals", () => {
    expect(elapsedLabel(24350, 0)).toBe("24.35s");
    expect(elapsedLabel(5000, 2000)).toBe("3.00s");
    expect(elapsedLabel(1_700_000_012_340, 1_700_000_000_000)).toBe("12.34s");
  });

  it("clamps a pre-origin timestamp to 0.00s", () => {
    expect(elapsedLabel(1000, 2000)).toBe("0.00s");
  });
});
