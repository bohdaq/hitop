// Keyboard shortcuts: one capture-phase listener on the document (so Mod+Enter
// works inside the CodeMirror editors, which bind it themselves), keyed on
// e.code for letters (Alt changes e.key on macOS). Browsers keep Ctrl/Cmd+T,
// +W and +N for themselves, so tab shortcuts use Alt+Shift; the same keys
// work in the desktop app.
export const IS_MAC = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent || "");

const MOD = IS_MAC ? "⌘" : "Ctrl";
const ALT = IS_MAC ? "⌥" : "Alt";

// [keys, what], in the order the help lists them.
export const SHORTCUTS = [
  [[MOD, "Enter"], "Send the request (from anywhere, editors included)"],
  [[MOD, "S"], "Save the request (a draft asks where)"],
  [[MOD, "K"], "Go to the URL field"],
  [["/"], "Go to the URL field (when not typing)"],
  [[ALT, "Shift", "N"], "New tab"],
  [[ALT, "Shift", "W"], "Close the tab"],
  [[ALT, "Shift", "]"], "Next tab"],
  [[ALT, "Shift", "["], "Previous tab"],
  [[MOD, "/"], "Show these shortcuts"],
  [["?"], "Show these shortcuts (when not typing)"],
  [["Esc", "Tab"], "Leave a code editor (Tab indents in it)"],
];

export const keyLabel = (keys) => keys.join(IS_MAC ? "" : "+");
export const SEND_KEYS = keyLabel([MOD, "Enter"]);
export const SAVE_KEYS = keyLabel([MOD, "S"]);

const typing = (e) =>
  e.composedPath().some((n) => n.localName === "input" || n.localName === "textarea" || n.localName === "select" || n.isContentEditable);

// The action for a keydown, or null. `actions`: send, save, focusUrl, newTab,
// closeTab, nextTab, prevTab, help.
export function shortcutFor(e) {
  if (e.isComposing || e.repeat) return null;
  const mod = IS_MAC ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
  if (mod && !e.altKey && !e.shiftKey) {
    if (e.key === "Enter") return "send";
    if (e.code === "KeyS") return "save";
    if (e.code === "KeyK") return "focusUrl";
    if (e.code === "Slash") return "help";
  }
  if (e.altKey && e.shiftKey && !e.ctrlKey && !e.metaKey) {
    if (e.code === "KeyN") return "newTab";
    if (e.code === "KeyW") return "closeTab";
    if (e.code === "BracketRight") return "nextTab";
    if (e.code === "BracketLeft") return "prevTab";
  }
  if (!e.ctrlKey && !e.metaKey && !e.altKey && !typing(e)) {
    if (e.key === "/") return "focusUrl";
    if (e.key === "?") return "help";
  }
  return null;
}
