/**
 * tNav.ts — type-safe navigation translation helper.
 *
 * Eliminates the `as any` cast in App.tsx and WorkspaceShell.tsx when
 * constructing i18n keys from ScreenKey values.  Provides a fully
 * typed variant so typos in screen keys are caught at compile time.
 */

import type { TFunction } from "i18next";
import type { ScreenKey } from "../types";

export type NavField = "title" | "kicker";

/**
 * Returns the translated navigation string for a given screen + field.
 * Falls back to the raw screen key if no translation exists.
 *
 * @example
 *   const title = tNav(t, "dashboard", "title"); // "Operator dashboard"
 */
export function tNav(t: TFunction, screen: ScreenKey, field: NavField): string {
  // Cast is isolated here, away from all call sites.
  return t(`navigation.${screen}.${field}` as Parameters<TFunction>[0], {
    defaultValue: screen,
  }) as string;
}

/**
 * Returns both title and kicker for a screen in one call.
 */
export function tNavMeta(
  t: TFunction,
  screen: ScreenKey
): { title: string; kicker: string } {
  return {
    title: tNav(t, screen, "title"),
    kicker: tNav(t, screen, "kicker"),
  };
}
