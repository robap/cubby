import { describe, it, expect } from "zero/test";
import {
  EXPIRY_OPTIONS,
  PREVIEW_MAX_BYTES,
  formatPreview,
  prettyJson,
  prettyXml,
  previewKind,
} from "./preview.ts";

describe("previewKind", () => {
  it("classifies images by the image/ prefix", () => {
    expect(previewKind("image/png", 1000)).toBe("image");
    expect(previewKind("image/jpeg", 1000)).toBe("image");
    expect(previewKind("image/svg+xml", 1000)).toBe("image");
  });

  it("classifies JSON as json (application/json and +json)", () => {
    expect(previewKind("application/json", 1000)).toBe("json");
    expect(previewKind("application/vnd.api+json", 1000)).toBe("json");
  });

  it("classifies text/* and common textual types as text", () => {
    expect(previewKind("text/plain", 1000)).toBe("text");
    expect(previewKind("text/markdown", 1000)).toBe("text");
    expect(previewKind("application/javascript", 1000)).toBe("text");
  });

  it("classifies XML as xml (application/xml, text/xml, +xml)", () => {
    expect(previewKind("application/xml", 1000)).toBe("xml");
    expect(previewKind("text/xml", 1000)).toBe("xml");
    expect(previewKind("application/atom+xml", 1000)).toBe("xml");
    // SVG is an image (the image/ prefix wins over the +xml suffix).
    expect(previewKind("image/svg+xml", 1000)).toBe("image");
  });

  it("falls back to none for unknown / binary types", () => {
    expect(previewKind("application/octet-stream", 1000)).toBe("none");
    expect(previewKind("video/mp4", 1000)).toBe("none");
    expect(previewKind(null, 1000)).toBe("none");
  });

  it("previews text/json up to the cap but not past it", () => {
    // Exactly at the cap still previews (boundary is strict `>`)…
    expect(previewKind("text/plain", PREVIEW_MAX_BYTES)).toBe("text");
    expect(previewKind("application/json", PREVIEW_MAX_BYTES)).toBe("json");
    // …one byte over does not.
    expect(previewKind("text/plain", PREVIEW_MAX_BYTES + 1)).toBe("none");
    expect(previewKind("application/json", PREVIEW_MAX_BYTES + 1)).toBe("none");
    // Images stream into an <img> tag, so size does not gate them.
    expect(previewKind("image/png", PREVIEW_MAX_BYTES + 1)).toBe("image");
  });
});

describe("prettyJson", () => {
  it("re-indents a minified object to multi-line", () => {
    const out = prettyJson('{"a":1,"b":[2,3]}');
    expect(out).toBe('{\n  "a": 1,\n  "b": [\n    2,\n    3\n  ]\n}');
    expect(out.split("\n").length).toBeGreaterThan(1);
  });

  it("returns malformed JSON unchanged (no throw, no blank)", () => {
    expect(prettyJson('{"a":1,')).toBe('{"a":1,');
    expect(prettyJson("not json at all")).toBe("not json at all");
  });
});

describe("prettyXml", () => {
  it("indents nested elements onto their own lines", () => {
    expect(prettyXml("<a><b>x</b></a>")).toBe("<a>\n  <b>\n    x\n  </b>\n</a>");
  });

  it("handles self-closing tags and a declaration", () => {
    expect(prettyXml('<?xml version="1.0"?><root><item/></root>')).toBe(
      '<?xml version="1.0"?>\n<root>\n  <item/>\n</root>',
    );
  });

  it("falls back to raw for mismatched tags", () => {
    expect(prettyXml("<a><b></a>")).toBe("<a><b></a>");
  });

  it("falls back to raw for unclosed tags", () => {
    expect(prettyXml("<a><b></b>")).toBe("<a><b></b>");
  });

  it("falls back to raw for count-balanced but interleaved (mismatched) tags", () => {
    // `</a>` must match the open `<b>` on the stack by *name*, not just depth.
    expect(prettyXml("<a><b></a></b>")).toBe("<a><b></a></b>");
  });

  it("falls back to raw for non-tag content", () => {
    expect(prettyXml("just some text")).toBe("just some text");
  });
});

describe("formatPreview", () => {
  it("pretty-prints by kind and leaves plain text alone", () => {
    expect(formatPreview("json", '{"a":1}')).toBe('{\n  "a": 1\n}');
    expect(formatPreview("xml", "<a>x</a>")).toBe("<a>\n  x\n</a>");
    expect(formatPreview("text", "line1\nline2")).toBe("line1\nline2");
  });
});

describe("EXPIRY_OPTIONS", () => {
  it("offers 5 min / 1 hour / 24 hours / 7 days in seconds", () => {
    const secs = EXPIRY_OPTIONS.map((o) => o.seconds);
    expect(secs).toEqual([300, 3600, 86400, 604800]);
  });

  it("labels each choice for the picker", () => {
    const labels = EXPIRY_OPTIONS.map((o) => o.label);
    expect(labels).toEqual(["5 minutes", "1 hour", "24 hours", "7 days"]);
  });
});
