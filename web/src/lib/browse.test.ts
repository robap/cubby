import { describe, it, expect } from "zero/test";
import { crumbs, folderLabel, highlightParts, uploadKey, viewMode } from "./browse.ts";

describe("viewMode", () => {
  it("is folder browsing when the search term is empty or whitespace", () => {
    expect(viewMode("")).toBe("folder");
    expect(viewMode("   ")).toBe("folder");
  });

  it("switches to the flat search list once the term has content", () => {
    expect(viewMode("report")).toBe("search");
    expect(viewMode("  port ")).toBe("search");
    // A single character already counts as content.
    expect(viewMode("a")).toBe("search");
  });
});

describe("crumbs", () => {
  it("returns just the bucket root at an empty prefix", () => {
    expect(crumbs("demo", "")).toEqual([{ label: "demo", prefix: "" }]);
  });

  it("builds cumulative prefixes for each segment", () => {
    expect(crumbs("demo", "a/b/")).toEqual([
      { label: "demo", prefix: "" },
      { label: "a", prefix: "a/" },
      { label: "b", prefix: "a/b/" },
    ]);
  });
});

describe("folderLabel", () => {
  it("strips the current prefix and keeps the trailing slash", () => {
    expect(folderLabel("a/b/", "a/")).toBe("b/");
    expect(folderLabel("css/", "")).toBe("css/");
  });
});

describe("uploadKey", () => {
  it("joins the current prefix with the file name", () => {
    expect(uploadKey("", "cat.jpg")).toBe("cat.jpg");
    expect(uploadKey("photos/", "cat.jpg")).toBe("photos/cat.jpg");
  });
});

describe("highlightParts", () => {
  it("splits a key around each case-insensitive match", () => {
    expect(highlightParts("landing-1x.png", "png")).toEqual([
      { text: "landing-1x.", match: false },
      { text: "png", match: true },
    ]);
  });

  it("marks a mid-key match and preserves surrounding text", () => {
    expect(highlightParts("a/report.pdf", "port")).toEqual([
      { text: "a/re", match: false },
      { text: "port", match: true },
      { text: ".pdf", match: false },
    ]);
  });

  it("returns the whole string unmatched when the term is empty or absent", () => {
    expect(highlightParts("abc", "")).toEqual([{ text: "abc", match: false }]);
    expect(highlightParts("abc", "zzz")).toEqual([{ text: "abc", match: false }]);
  });

  it("emits no empty leading run when the match is at the very start", () => {
    expect(highlightParts("report", "rep")).toEqual([
      { text: "rep", match: true },
      { text: "ort", match: false },
    ]);
  });

  it("returns a single empty unmatched run for an empty key", () => {
    expect(highlightParts("", "x")).toEqual([{ text: "", match: false }]);
  });
});
