// Store for the app chrome: the health payload behind the top bar and nav
// footer, plus the light/dark theme toggle. Module-level signals are a store.

import { signal } from "zero";
import type { Health } from "../lib/api.ts";
import { getHealth } from "../lib/api.ts";

/** The latest health payload, or `null` before the first load. */
export const health = signal<Health | null>(null);

/** Whether the last health poll succeeded (drives the status dot). */
export const healthy = signal<boolean>(false);

/**
 * Fetch health once and update the store. Safe to call repeatedly.
 * @returns {Promise<void>}
 */
export async function loadHealth(): Promise<void> {
  try {
    const h = await getHealth();
    health.set(h);
    healthy.set(h.status === "ok");
  } catch {
    healthy.set(false);
  }
}

/** The theme *preference*: an explicit override, or follow the OS. */
export type ThemePref = "dark" | "light" | "system";

/** The effective theme actually applied to `<html>`. */
export type Theme = "dark" | "light";

/** localStorage key for the persisted preference. Shared with the inline
 * no-flash bootstrap in `index.html` — keep the two in sync. */
export const THEME_KEY = "cubby:theme";

/** The cycle order of the three-state theme control. */
const THEME_ORDER: ThemePref[] = ["dark", "light", "system"];

/** The current theme preference (persisted). Default `system` for a fresh browser. */
export const themePref = signal<ThemePref>(readStoredPref());

/**
 * The next preference in the cycle: dark → light → system → dark.
 * @param {ThemePref} pref
 * @returns {ThemePref}
 */
export function nextThemePref(pref: ThemePref): ThemePref {
  return THEME_ORDER[(THEME_ORDER.indexOf(pref) + 1) % THEME_ORDER.length]!;
}

/**
 * Resolve a preference to the effective theme: explicit overrides win; `system`
 * follows the OS.
 * @param {ThemePref} pref
 * @param {boolean} systemPrefersDark
 * @returns {Theme}
 */
export function resolveTheme(pref: ThemePref, systemPrefersDark: boolean): Theme {
  if (pref === "dark" || pref === "light") return pref;
  return systemPrefersDark ? "dark" : "light";
}

/** Whether the OS currently prefers a dark scheme (defaults to dark if unknown). */
function systemPrefersDark(): boolean {
  return !(typeof matchMedia === "function" && matchMedia("(prefers-color-scheme: light)").matches);
}

/** The effective theme for the current preference + OS setting. */
export function effectiveTheme(): Theme {
  return resolveTheme(themePref.val, systemPrefersDark());
}

/**
 * Coerce a stored value to a valid preference, defaulting to `system` when it is
 * absent or unrecognized (a brand-new browser matches its environment).
 * @param {string | null} v
 * @returns {ThemePref}
 */
export function parseThemePref(v: string | null): ThemePref {
  return v === "dark" || v === "light" || v === "system" ? v : "system";
}

/**
 * Read the persisted preference (or the `system` default).
 * @returns {ThemePref}
 */
function readStoredPref(): ThemePref {
  return parseThemePref(safeLocalGet(THEME_KEY));
}

/** localStorage getter that tolerates a disabled/absent store. */
function safeLocalGet(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

/** localStorage setter that tolerates a disabled/absent store. */
function safeLocalSet(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* storage unavailable (private mode, tests) — the signal still drives the UI */
  }
}

/**
 * Set the theme preference: persist it and stamp the effective theme on
 * `<html data-theme>`.
 * @param {ThemePref} pref
 * @returns {void}
 */
export function setThemePref(pref: ThemePref): void {
  themePref.set(pref);
  safeLocalSet(THEME_KEY, pref);
  applyTheme();
}

/** Advance the preference one step through the cycle and apply it. */
export function cycleTheme(): void {
  setThemePref(nextThemePref(themePref.val));
}

/** Stamp the effective theme on `<html>` (startup and on every change). */
export function applyTheme(): void {
  document.documentElement.setAttribute("data-theme", effectiveTheme());
}

/**
 * Reflect live OS `prefers-color-scheme` changes while the preference is
 * `system`. Call once at startup.
 * @returns {void}
 */
export function watchSystemTheme(): void {
  if (typeof matchMedia !== "function") return;
  matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
    if (themePref.val === "system") applyTheme();
  });
}
