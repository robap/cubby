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

/** The active theme (`"dark"` is the hero). */
export const theme = signal<"dark" | "light">(readInitialTheme());

/**
 * Resolve the initial theme from an explicit `data-theme` or the OS setting.
 * @returns {"dark" | "light"}
 */
function readInitialTheme(): "dark" | "light" {
  const attr = document.documentElement.getAttribute("data-theme");
  if (attr === "light" || attr === "dark") return attr;
  const prefersLight =
    typeof matchMedia === "function" && matchMedia("(prefers-color-scheme: light)").matches;
  return prefersLight ? "light" : "dark";
}

/** Flip the theme and stamp it on `<html data-theme>`. */
export function toggleTheme(): void {
  const next = theme.val === "dark" ? "light" : "dark";
  theme.set(next);
  document.documentElement.setAttribute("data-theme", next);
}

/** Apply the resolved theme to `<html>` at startup. */
export function applyTheme(): void {
  document.documentElement.setAttribute("data-theme", theme.val);
}
