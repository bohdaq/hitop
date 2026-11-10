// The app is dark only (owner decision 2026-11-09, HITOP_NEXT_UI.md): index.html's boot script sets
// color-scheme="dark" on <html> before first paint and theme.css adds `color-scheme: dark`, so
// every light-dark() token resolves to its dark side. Earlier builds kept a theme choice under
// `hitop-theme` / `hitop-theme-system`; nothing reads them any more, so they are removed.

export function forgetThemeChoice() {
  for (const key of ["hitop-theme", "hitop-theme-system"]) {
    try {
      localStorage.removeItem(key);
    } catch {
      // Storage unavailable: nothing to forget.
    }
  }
  document.documentElement.setAttribute("color-scheme", "dark");
}
