// HITOP Next: the app is dark by default (owner decision 2026-11-09, HITOP_NEXT_UI.md).
// `hitop-theme` keeps a forced "light" or "dark" (lit-material's color-scheme helpers).
// With nothing saved the boot script in index.html forces dark, unless the person picked
// "Match the system", which @lit-material/color-scheme records by removing `hitop-theme`;
// that choice is kept under `hitop-theme-system` so it survives a reload.

import { COLOR_SCHEME_EVENT, storedColorScheme } from "@lit-material/color-scheme";

export const THEME_KEY = "hitop-theme";
export const SYSTEM_KEY = "hitop-theme-system";

function followsSystem() {
  try {
    return localStorage.getItem(SYSTEM_KEY) === "1";
  } catch {
    return false;
  }
}

/** The person's choice as the Settings select shows it: light, dark or auto (Match the system). */
export function appScheme() {
  const stored = storedColorScheme(THEME_KEY);
  if (stored !== "auto") return stored;
  return followsSystem() ? "auto" : "dark";
}

/** Keeps `hitop-theme-system` in step with every theme change (top bar toggle, Settings). */
export function watchScheme() {
  document.addEventListener(COLOR_SCHEME_EVENT, (e) => {
    try {
      if (e.detail?.scheme === "auto") localStorage.setItem(SYSTEM_KEY, "1");
      else localStorage.removeItem(SYSTEM_KEY);
    } catch {
      // Storage unavailable: the choice lasts for this page only.
    }
  });
}
