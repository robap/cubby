// Rendering tests for the read-only per-bucket CORS panel. Pure-render
// assertions seed the cors store directly. There is no add/edit/delete control —
// the panel is display only (management is the S3 API).

import {
  describe, it, expect, beforeEach, afterEach, render, find, findAll, text, cleanup,
} from "zero/test";
import type { CorsInfo } from "../lib/api.ts";
import CorsPanel from "./cors-panel.ts";
import { loadedBucket, panelOpen, rules } from "../stores/cors.ts";
import { selectedBucket } from "../stores/browse.ts";

function resetStore(): void {
  panelOpen.set(false);
  rules.set(null);
  loadedBucket.set(null);
  selectedBucket.set("uploads");
}

const rule = (over: Partial<CorsInfo> = {}): CorsInfo => ({
  AllowedOrigins: ["http://localhost:3000"],
  AllowedMethods: ["GET", "PUT"],
  AllowedHeaders: ["*"],
  ExposeHeaders: ["ETag"],
  MaxAgeSeconds: 600,
  ...over,
});

describe("CorsPanel", () => {
  beforeEach(resetStore);
  afterEach(cleanup);

  it("renders each rule's origins, methods, headers, expose-headers, and max-age", () => {
    rules.set([rule({ ExposeHeaders: ["ETag", "Content-Length"] })]);
    const el = render(CorsPanel());
    const rows = findAll(el, ".cors-rule");
    expect(rows.length).toBe(1);
    const t = text(el, ".cors-rule");
    // Labels are present…
    expect(t).toContain("Origins");
    expect(t).toContain("Methods");
    expect(t).toContain("Allowed headers");
    expect(t).toContain("Expose headers");
    expect(t).toContain("Max-Age");
    // …and values are comma-joined (a dropped separator would read "GETPUT").
    expect(t).toContain("http://localhost:3000");
    expect(t).toContain("GET, PUT");
    expect(t).toContain("ETag, Content-Length");
    expect(t).toContain("600s");
  });

  it("renders multiple rules", () => {
    rules.set([
      rule({ AllowedOrigins: ["http://a"] }),
      rule({ AllowedOrigins: ["http://b"] }),
    ]);
    const el = render(CorsPanel());
    expect(findAll(el, ".cors-rule").length).toBe(2);
  });

  it("shows the 'no CORS configured' empty state when rules is null", () => {
    rules.set(null);
    const el = render(CorsPanel());
    expect(findAll(el, ".cors-rule").length).toBe(0);
    expect(text(el, ".cors-empty").toLowerCase()).toContain("no cors");
  });

  it("omits absent optional fields whether undefined or empty", () => {
    // Only origins + methods should render — exactly two fields, no headers/
    // expose/max-age labels. Covers both `undefined` and empty-array cases so the
    // `length > 0` guards can't degrade to showing an empty field.
    for (const over of [
      { AllowedHeaders: undefined, ExposeHeaders: undefined, MaxAgeSeconds: undefined },
      { AllowedHeaders: [], ExposeHeaders: [], MaxAgeSeconds: undefined },
    ] as Partial<CorsInfo>[]) {
      rules.set([rule(over)]);
      const el = render(CorsPanel());
      expect(findAll(el, ".cors-field").length).toBe(2);
      const t = text(el, ".cors-rule");
      expect(t).toContain("Origins");
      expect(t).toContain("Methods");
      expect(t).not.toContain("Allowed headers");
      expect(t).not.toContain("Expose headers");
      expect(t).not.toContain("Max-Age");
      cleanup();
    }
  });

  it("offers no add/edit/delete control (display only)", () => {
    rules.set([rule()]);
    const el = render(CorsPanel());
    expect(find(el, "form")).toBe(null);
    expect(find(el, ".row-delete")).toBe(null);
    expect(find(el, "input")).toBe(null);
  });
});
