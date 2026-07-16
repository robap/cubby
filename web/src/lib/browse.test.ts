import { describe, it, expect } from "zero/test";
import {
  crumbs,
  folderLabel,
  highlightParts,
  locationToUrl,
  parentPrefix,
  uploadKey,
  urlToLocation,
  viewMode,
} from "./browse.ts";
import type { BrowseLocation } from "./browse.ts";

/** Parse a URL's query the way the router does (decoded key→value map). */
function parseQuery(url: string): Record<string, string> {
  const out: Record<string, string> = {};
  const q = url.indexOf("?");
  if (q < 0) return out;
  for (const [k, v] of new URLSearchParams(url.slice(q + 1))) out[k] = v;
  return out;
}

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

describe("parentPrefix", () => {
  it("returns the folder up to and including the last slash", () => {
    expect(parentPrefix("a/b/c.txt")).toBe("a/b/");
    expect(parentPrefix("my docs/2026/report.pdf")).toBe("my docs/2026/");
  });

  it("returns empty for a top-level key", () => {
    expect(parentPrefix("readme.md")).toBe("");
  });

  it("keeps a leading-slash key's own slash as the prefix", () => {
    // A key whose first char is `/` has its slash at index 0 — the boundary
    // that separates a folder ("/") from a top-level key ("").
    expect(parentPrefix("/rooted.txt")).toBe("/");
  });
});

describe("locationToUrl", () => {
  it("is the bare browser URL when no bucket is selected", () => {
    expect(locationToUrl({ bucket: null, prefix: "", object: null })).toBe("/_/browser");
  });

  it("carries just the bucket at the root", () => {
    expect(locationToUrl({ bucket: "demo", prefix: "", object: null })).toBe(
      "/_/browser?bucket=demo",
    );
  });

  it("carries a folder prefix, percent-encoding slashes and spaces as %20", () => {
    const url = locationToUrl({ bucket: "my docs", prefix: "a b/2026/", object: null });
    expect(url).toBe("/_/browser?bucket=my%20docs&prefix=a%20b%2F2026%2F");
    expect(url).not.toContain("+");
  });

  it("carries an object key and omits any prefix (derived on the way back)", () => {
    const url = locationToUrl({ bucket: "demo", prefix: "a/", object: "a/report v2.pdf" });
    expect(url).toBe("/_/browser?bucket=demo&object=a%2Freport%20v2.pdf");
    expect(url).not.toContain("prefix=");
  });
});

describe("urlToLocation", () => {
  it("is the default landing when there is no bucket", () => {
    expect(urlToLocation({})).toEqual({ bucket: null, prefix: "", object: null });
  });

  it("reads a folder prefix", () => {
    expect(urlToLocation({ bucket: "demo", prefix: "a/b/" })).toEqual({
      bucket: "demo",
      prefix: "a/b/",
      object: null,
    });
  });

  it("derives an open object's prefix from its key", () => {
    expect(urlToLocation({ bucket: "demo", object: "a/b/c.txt" })).toEqual({
      bucket: "demo",
      prefix: "a/b/",
      object: "a/b/c.txt",
    });
  });
});

describe("browser location round-trips through a URL", () => {
  const cases: BrowseLocation[] = [
    { bucket: null, prefix: "", object: null },
    { bucket: "demo", prefix: "", object: null },
    { bucket: "my docs", prefix: "my docs/2026/", object: null },
    { bucket: "demo", prefix: "a/b/", object: "a/b/report v2.pdf" },
  ];
  for (const loc of cases) {
    it(`round-trips ${JSON.stringify(loc)}`, () => {
      // An open object drops the stored prefix in the URL, then re-derives it —
      // so the expected round-trip prefix is the object's own parent.
      const expected =
        loc.object !== null ? { ...loc, prefix: parentPrefix(loc.object) } : loc;
      expect(urlToLocation(parseQuery(locationToUrl(loc)))).toEqual(expected);
    });
  }
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
