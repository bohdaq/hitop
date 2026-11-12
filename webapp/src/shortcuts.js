// Keyboard shortcuts: one capture-phase listener on the document (so Mod+Enter
// works inside the CodeMirror editors, which bind it themselves), keyed on
// e.code for letters (Alt changes e.key on macOS). Browsers keep Ctrl/Cmd+T,
// +W and +N for themselves, so tab shortcuts use Alt+Shift; the same keys
// work in the desktop app. Mod+/ is left to the code editors (toggle comment).
// Mod+K opens the command palette (HITOP Next A5), Mod+L goes to the URL field (as in a browser),
// Mod+P opens quick open (find any request).
export const IS_MAC = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent || "");

const MOD = IS_MAC ? "⌘" : "Ctrl";
const ALT = IS_MAC ? "⌥" : "Alt";

// [keys, what], in the order the help lists them.
export const SHORTCUTS = [
  [[MOD, "Enter"], "Send the request (from anywhere, editors included)"],
  [[MOD, "S"], "Save the request"],
  [[MOD, "K"], "Command palette: requests, commands, environments, runs"],
  [[MOD, "L"], "Go to the URL field"],
  [[MOD, "P"], "Quick open: find any saved request or open tab (in place of Print)"],
  [[MOD, "B"], "Show or hide the side pane"],
  [["/"], "Go to the URL field (when not typing)"],
  [[ALT, "Shift", "N"], "New tab"],
  [[ALT, "Shift", "W"], "Close the tab"],
  [[ALT, "Shift", "]"], "Next tab"],
  [[ALT, "Shift", "["], "Previous tab"],
  [["?"], "Show these shortcuts (when not typing)"],
  [["Esc"], "Cancel the request in flight; on a page, back to the request"],
  [[ALT, "Shift", "E"], "Pick the environment"],
  [[ALT, "Shift", "F"], "Filter the sidebar's requests"],
  [[ALT, "Shift", "R"], "Go to the response's tabs"],
  [["↑", "↓", "←", "→"], "In the sidebar: move, open and fold (F2 renames, Delete deletes, Shift+F10 opens the menu, Alt+↑/↓ moves)"],
  [["Esc", "Tab"], "Leave a code editor (Tab indents in it)"],
];

export const keyLabel = (keys) => keys.join(IS_MAC ? "" : "+");
export const SEND_KEYS = keyLabel([MOD, "Enter"]);
export const SAVE_KEYS = keyLabel([MOD, "S"]);

/** Each action's keys as the palette shows them (from SHORTCUTS, so the two never disagree). */
export const ACTION_KEYS = {
  send: [MOD, "Enter"],
  save: [MOD, "S"],
  focusUrl: [MOD, "L"],
  newTab: [ALT, "Shift", "N"],
  closeTab: [ALT, "Shift", "W"],
  help: ["?"],
  sidePane: [MOD, "B"],
  environment: [ALT, "Shift", "E"],
  filter: [ALT, "Shift", "F"],
};

const typing = (e) =>
  e.composedPath().some((n) => n.localName === "input" || n.localName === "textarea" || n.localName === "select" || n.isContentEditable);

// The action for a keydown, or null. `actions`: send, save, palette, focusUrl,
// quickOpen, newTab, closeTab, nextTab, prevTab, help. Mod+P is the
// browser's Print elsewhere: the app takes it only while it has the focus.
export function shortcutFor(e) {
  if (e.isComposing || e.repeat) return null;
  const mod = IS_MAC ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
  if (mod && !e.altKey && !e.shiftKey) {
    if (e.key === "Enter") return "send";
    if (e.code === "KeyS") return "save";
    if (e.code === "KeyK") return "palette";
    if (e.code === "KeyL") return "focusUrl";
    if (e.code === "KeyP") return "quickOpen";
    if (e.code === "KeyB") return "sidePane";
  }
  if (e.altKey && e.shiftKey && !e.ctrlKey && !e.metaKey) {
    if (e.code === "KeyN") return "newTab";
    if (e.code === "KeyW") return "closeTab";
    if (e.code === "BracketRight") return "nextTab";
    if (e.code === "BracketLeft") return "prevTab";
    if (e.code === "KeyE") return "environment";
    if (e.code === "KeyF") return "filter";
    if (e.code === "KeyR") return "responseTabs";
  }
  if (!e.ctrlKey && !e.metaKey && !e.altKey && !typing(e)) {
    if (e.key === "/") return "focusUrl";
    if (e.key === "?") return "help";
  }
  return null;
}
