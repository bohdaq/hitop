// What the parts of the app shell share.

import { asksForEvents } from "../../features.js";

/** Email addresses typed in the invite dialog: comma, space or line separated. */
export function inviteEmails(text) {
  return [...new Set(String(text).split(/[\s,;]+/).map((e) => e.trim()).filter((e) => /^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(e)))];
}

// The HITOP Cloud portal (plan, organizations, billing): web only, never in the desktop build (App
// Store: no purchase links).
export const PORTAL_URL = "https://cloud.hitop.rws8.tech/";
// The org switcher's last option: opens the New organization dialog.
export const NEW_ORG = "__new_org";
// The environment picker's "Add environment…" (FV1, shown when there is none).
export const ADD_ENVIRONMENT = "__add_environment";

// Collections folded in the sidebar, by id (its own key: nothing else reads it).
export const COLLAPSED_KEY = "hitop_collapsed_collections";
// The save dialog's collection picker: this value makes a new collection from the name typed next to it.
export const NEW_COLLECTION = "__new__";
export function loadCollapsed() {
  try {
    const v = JSON.parse(localStorage.getItem(COLLAPSED_KEY) || "[]");
    return Array.isArray(v) ? v.map(String) : [];
  } catch {
    return [];
  }
}
export function saveCollapsed(ids) {
  try {
    localStorage.setItem(COLLAPSED_KEY, JSON.stringify(ids));
  } catch {
    // storage full or blocked: folding lasts for this page
  }
}

// Import errors in words: a parser's message ("Unexpected token } in JSON at
// position 41") tells most people nothing.
export function readableError(err, what) {
  if (err instanceof SyntaxError) return `That isn't valid JSON. Paste the whole of ${what}, from the first { or [ to the last.`;
  return `That isn't ${what} HITOP can read: ${err.message}`;
}

// A menu for a right click (or the context menu key). Where the menu opens
// while the right button is still down (Linux, macOS), the release would
// dismiss it at once (popover light dismiss): it opens after the release.
// The method tag of an open tab: WS, GRPC, GQL (a GraphQL body) or the HTTP method.
export const tagOf = (t) => (t.kind === "websocket" ? "WS" : t.kind === "grpc" ? "GRPC" : t.bodyMode === "graphql" ? "GQL" : (t.method || "GET") === "GET" && asksForEvents(t.headers) ? "SSE" : t.method || "GET");

export function showMenuFor(e, menu) {
  const anchor = e.currentTarget;
  if (!(e.buttons & 2)) return menu.show(anchor);
  addEventListener("pointerup", () => setTimeout(() => menu.show(anchor)), { once: true, capture: true });
}

// The palette preview renders inside the palette's shadow root, so it brings its own styles.
export const PREVIEW_CSS = `
  .palette-preview { display: grid; gap: 10px; font-size: 12.5px; color: #c8c8cf; }
  .mono { font-family: var(--font-mono, "Geist Mono", ui-monospace, monospace); }
  .pp-line { display: flex; align-items: center; gap: 10px; min-width: 0; }
  .pp-url { color: #ededf0; overflow-wrap: anywhere; }
  .pp-place { color: #8a8a94; font-size: 11.5px; }
  .pp-label { margin-top: 4px; font-size: 11px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: #8a8a94; }
  .pp-headers { display: grid; grid-template-columns: max-content 1fr; gap: 2px 12px; margin: 0; }
  .pp-headers dt { color: #a1a1aa; }
  .pp-headers dd { margin: 0; overflow-wrap: anywhere; }
  .pp-body { margin: 0; padding: 8px 10px; border: 1px solid #26262c; border-radius: 8px; background: #0f0f12; white-space: pre-wrap; overflow-wrap: anywhere; line-height: 19px; }
`;
