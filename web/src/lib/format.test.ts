import { describe, it, expect } from "zero/test";
import {
  baseName,
  bytesCell,
  fmtDate,
  groupDigits,
  humanBytes,
  middleTruncate,
  statusClass,
  targetOf,
  truncateEnd,
} from "./format.ts";

describe("groupDigits", () => {
  it("inserts thousands separators", () => {
    expect(groupDigits(984_221)).toBe("984,221");
    expect(groupDigits(1_000_000)).toBe("1,000,000");
  });

  it("leaves sub-thousand numbers unchanged", () => {
    expect(groupDigits(0)).toBe("0");
    expect(groupDigits(512)).toBe("512");
  });
});

describe("humanBytes", () => {
  it("scales through units", () => {
    expect(humanBytes(0)).toBe("0 B");
    expect(humanBytes(512)).toBe("512 B");
    expect(humanBytes(2048)).toBe("2.0 KB");
    expect(humanBytes(2_516_582)).toBe("2.4 MB");
    expect(humanBytes(5 * 1024 ** 3)).toBe("5.0 GB");
    expect(humanBytes(5 * 1024 ** 4)).toBe("5.0 TB");
    expect(humanBytes(5 * 1024 ** 5)).toBe("5.0 PB");
  });

  it("promotes exactly at each 1024 boundary and caps at PB", () => {
    // 1024² bytes is exactly 1024 KB → promotes to 1.0 MB (boundary is `>=`).
    expect(humanBytes(1024 ** 2)).toBe("1.0 MB");
    // Beyond PB there is no larger unit — it stays in PB, not overflow.
    expect(humanBytes(1024 ** 6)).toBe("1024.0 PB");
  });

  it("returns an em dash for invalid sizes", () => {
    expect(humanBytes(-1)).toBe("—");
    expect(humanBytes(NaN)).toBe("—");
  });
});

describe("statusClass", () => {
  it("maps each class", () => {
    expect(statusClass(204)).toBe("ok");
    expect(statusClass(301)).toBe("redirect");
    expect(statusClass(403)).toBe("warn");
    expect(statusClass(500)).toBe("err");
  });

  it("colors exactly at each class boundary (inclusive lower edge)", () => {
    expect(statusClass(299)).toBe("ok");
    expect(statusClass(300)).toBe("redirect");
    expect(statusClass(399)).toBe("redirect");
    expect(statusClass(400)).toBe("warn");
    expect(statusClass(499)).toBe("warn");
    expect(statusClass(500)).toBe("err");
  });
});

describe("bytesCell", () => {
  it("shows an up arrow for request bytes, down for response", () => {
    expect(bytesCell({ bytes_in: 2_516_582, bytes_out: 0 })).toBe("↑ 2.4 MB");
    expect(bytesCell({ bytes_in: 0, bytes_out: 1024 })).toBe("↓ 1.0 KB");
    expect(bytesCell({ bytes_in: 0, bytes_out: 0 })).toBe("—");
  });

  it("treats a single byte as present (the arrow shows from 1, not just >1)", () => {
    expect(bytesCell({ bytes_in: 1, bytes_out: 0 })).toBe("↑ 1 B");
    expect(bytesCell({ bytes_in: 0, bytes_out: 1 })).toBe("↓ 1 B");
  });
});

describe("targetOf", () => {
  it("joins bucket and key, or falls back", () => {
    expect(targetOf({ bucket: "demo", key: "a/b" })).toBe("demo/a/b");
    expect(targetOf({ bucket: "demo", key: null })).toBe("demo");
    expect(targetOf({ bucket: null, key: null })).toBe("—");
  });
});

describe("middleTruncate", () => {
  it("keeps both ends of a long key", () => {
    const out = middleTruncate("app-assets/images/hero/landing-1x.png", 20);
    expect(out.length).toBeLessThanOrEqual(20);
    expect(out.startsWith("app-")).toBe(true);
    expect(out.endsWith(".png")).toBe(true);
    expect(out).toContain("…");
  });

  it("leaves short keys untouched", () => {
    expect(middleTruncate("a/b.txt", 20)).toBe("a/b.txt");
  });

  it("does not truncate a key that is exactly the max length", () => {
    expect(middleTruncate("abcde", 5)).toBe("abcde");
  });
});

describe("truncateEnd", () => {
  it("keeps the head and appends an ellipsis when too long", () => {
    expect(truncateEnd("5730185b04808598d667cf03f6a7c16a", 10)).toBe("5730185b04…");
  });

  it("leaves a short value untouched", () => {
    expect(truncateEnd("abcd", 10)).toBe("abcd");
  });

  it("does not truncate a value that is exactly the max length", () => {
    expect(truncateEnd("abcde", 5)).toBe("abcde");
  });
});

describe("fmtDate", () => {
  it("formats an ISO string and handles empties", () => {
    expect(fmtDate("2026-07-09T14:22:00Z")).toContain("2026-07-09");
    expect(fmtDate(null)).toBe("—");
    expect(fmtDate("not-a-date")).toBe("—");
  });
});

describe("baseName", () => {
  it("returns the last path segment", () => {
    expect(baseName("a/b/c.txt")).toBe("c.txt");
    expect(baseName("top.txt")).toBe("top.txt");
    expect(baseName("folder/")).toBe("folder");
  });

  it("handles a leading-slash key (slash at index 0)", () => {
    expect(baseName("/file.txt")).toBe("file.txt");
  });
});
