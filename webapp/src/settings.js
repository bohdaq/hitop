// The app's own settings (the Settings page), in localStorage `hitop_settings`
// (a key nothing else reads), in both builds:
//   timeoutMs   request time limit for Send and collection runs; 0 = none
//   syncServer  the sync server the Sign in dialog uses (absent = HITOP Cloud's), kept after a
//               sign-in on another one (sign-in-dialog.js)
// Layout choices live in their own keys (split.js); the app is dark only (theme.js).
// Unknown keys are kept.
const KEY = "hitop_settings";

export const DEFAULT_SETTINGS = { timeoutMs: 0 };

export function loadSettings() {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) || "{}");
    if (!v || typeof v !== "object" || Array.isArray(v)) return { ...DEFAULT_SETTINGS };
    const out = { ...DEFAULT_SETTINGS, ...v };
    out.timeoutMs = Number.isFinite(out.timeoutMs) && out.timeoutMs > 0 ? Math.round(out.timeoutMs) : 0;
    return out;
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

export function saveSettings(settings) {
  try {
    localStorage.setItem(KEY, JSON.stringify(settings));
    return true;
  } catch {
    // storage full or blocked: the setting lasts for this page
    return false;
  }
}

// "30 s", "1.5 s", "2 min" for the timeout notes.
export function seconds(ms) {
  if (ms >= 60_000 && ms % 60_000 === 0) return `${ms / 60_000} min`;
  return `${Math.round(ms / 100) / 10} s`;
}
