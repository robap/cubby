// Theme-preference logic: the three-state cycle order, the pref→effective-theme
// resolution, and that cycling advances the store + stamps `<html data-theme>`.

import { describe, it, expect, beforeEach } from "zero/test";
import {
  cycleTheme,
  effectiveTheme,
  nextThemePref,
  parseThemePref,
  resolveTheme,
  setThemePref,
  themePref,
} from "./chrome.ts";

describe("parseThemePref", () => {
  it("accepts each valid preference", () => {
    expect(parseThemePref("dark")).toBe("dark");
    expect(parseThemePref("light")).toBe("light");
    expect(parseThemePref("system")).toBe("system");
  });

  it("defaults to system for missing or unrecognized values", () => {
    expect(parseThemePref(null)).toBe("system");
    expect(parseThemePref("")).toBe("system");
    expect(parseThemePref("purple")).toBe("system");
  });
});

describe("nextThemePref", () => {
  it("cycles dark → light → system → dark", () => {
    expect(nextThemePref("dark")).toBe("light");
    expect(nextThemePref("light")).toBe("system");
    expect(nextThemePref("system")).toBe("dark");
  });
});

describe("resolveTheme", () => {
  it("honors explicit overrides regardless of the OS", () => {
    expect(resolveTheme("dark", false)).toBe("dark");
    expect(resolveTheme("dark", true)).toBe("dark");
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("light", false)).toBe("light");
  });

  it("follows the OS in system mode", () => {
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
  });
});

describe("cycleTheme", () => {
  beforeEach(() => setThemePref("dark"));

  it("advances the stored preference through the cycle and back", () => {
    cycleTheme();
    expect(themePref.val).toBe("light");
    cycleTheme();
    expect(themePref.val).toBe("system");
    cycleTheme();
    expect(themePref.val).toBe("dark");
  });

  it("stamps the effective theme on <html data-theme>", () => {
    setThemePref("light");
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
    expect(effectiveTheme()).toBe("light");
  });
});
