import { describe, it, expect } from "zero/test";
import { EXPIRY_OPTIONS, PREVIEW_MAX_BYTES, previewKind } from "./preview.ts";

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
    expect(previewKind("application/xml", 1000)).toBe("text");
    expect(previewKind("application/javascript", 1000)).toBe("text");
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
