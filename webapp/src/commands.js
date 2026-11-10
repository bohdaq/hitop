// The command palette's contents (Mod+K, HITOP Next A5): one registry of actions, plus the
// requests, environments and history runs the store has. Pure apart from the `app` calls in
// each action's `run`, so the items are easy to check.

// Sections and their prefixes: ">" only commands, "@" only environments, "#" only runs.
export const SECTIONS = { requests: "Requests", actions: "Actions", environments: "Environments", runs: "Runs" };
export const PREFIXES = { ">": SECTIONS.actions, "@": SECTIONS.environments, "#": SECTIONS.runs };

/**
 * Every app action the palette offers, once: [id, label, run(app), options]. The shortcut keys come
 * from shortcuts.js (SHORTCUTS) so the palette and the shortcuts list never disagree.
 */
export function actions(app, shortcuts = {}) {
  const s = app.store;
  const onRequest = s.view.kind === "request";
  const saved = onRequest ? s.loadedRequest() : null;
  const draft = onRequest && !saved;
  const context = s.visibleCollections.length ? s.contextCollection() : null;
  const list = [
    ["new-request", context ? `New request in ${context.name}` : "New request", () => s.newTab(), { keywords: ["tab", "create"], shortcut: shortcuts.newTab }],
    ["new-collection", "New collection", () => app.openDialog("add-collection"), { keywords: ["create", "folder", "group"] }],
    ["new-folder", context ? `New folder in ${context.name}` : "New folder", () => app.openDialog("new-folder", { collectionId: context?.id, parent: "" }), { keywords: ["create", "directory"], disabled: !context }],
    ["send", "Send the request", () => s.send(), { disabled: !onRequest || !s.tab?.url, shortcut: shortcuts.send }],
    ["save", draft ? "Save the request to a collection…" : "Save the request", () => app.saveCurrent(), { disabled: !onRequest, shortcut: shortcuts.save }],
    ["move-request", "Move the request to a collection or folder…", () => app.openDialog("move-request", { request: saved, collectionId: s.tab.loadedCollectionId }), { keywords: ["folder", "collection"], disabled: !saved }],
    ["focus-url", "Go to the URL field", () => app.focusUrl(), { shortcut: shortcuts.focusUrl }],
    ["close-tab", "Close the tab", () => app.closeTab(s.currentTab), { disabled: !onRequest || s.tabs.length < 2, shortcut: shortcuts.closeTab }],
    ["ask-ai", "Ask AI about this request", () => app.openAi(), { keywords: ["agent", "tests", "explain"], disabled: !onRequest }],
    ["code", "Copy as code (curl, fetch, Python…)", () => app.openDialog("code", { lang: "curl", showSecrets: false }), { keywords: ["snippet", "curl"], disabled: !onRequest || !s.tab?.url }],
    ["compare", "Compare responses", () => app.openDialog("compare"), { keywords: ["diff"], disabled: !onRequest }],
    ["import", "Import (Postman, OpenAPI, HITOP)", () => app.openDialog("import"), { keywords: ["postman", "openapi"] }],
    ["export", "Export collections", () => app.openDialog("export") ],
    ["environments", "Manage environments", () => app.go("environments"), { keywords: ["variables", "secrets"] }],
    ["history", "Open history", () => app.go("history") ],
    ["collections", "Show all collections", () => app.go("collections") ],
    ["cookies", "Cookies", () => app.go("cookies") ],
    ["settings", "Settings", () => app.go("settings"), { keywords: ["theme", "preferences"] }],
    ["shortcuts", "Keyboard shortcuts", () => app.openDialog("shortcuts"), { shortcut: shortcuts.help }],
    ["account", app.account() ? "Account & devices" : "Sign in to HITOP Cloud", () => app.go("sync"), { keywords: ["login", "sync"] }],
  ];
  return list.map(([id, label, run, opts = {}]) => ({ id: `action:${id}`, label, section: SECTIONS.actions, run, ...opts }));
}

/** Requests, open tabs, environments and recent runs as palette items, each with its own `run`. */
export function storeItems(app, methodLabel) {
  const s = app.store;
  const out = [];
  s.tabs.forEach((t, i) => {
    if (t.loadedRequestId) return;
    out.push({ id: `tab:${t.id}`, label: t.title || "New request", section: SECTIONS.requests, hint: "open tab", run: () => s.selectTab(i) });
  });
  for (const c of s.visibleCollections) {
    for (const r of c.requests || []) {
      out.push({
        id: `request:${c.id}:${r.id}`,
        label: `${methodLabel(r)} ${r.name}`,
        section: SECTIONS.requests,
        hint: c.name,
        keywords: [r.url ?? "", r.file ?? ""],
        run: () => s.openRequest(r, c.id),
      });
    }
  }
  for (const e of s.visibleEnvironments) {
    const active = s.activeEnvironment?.id === e.id;
    out.push({ id: `env:${e.id}`, label: `Use ${s.environmentLabel(e)}`, section: SECTIONS.environments, hint: active ? "active" : "", run: () => s.setActiveEnvironment(e.id) });
  }
  out.push({ id: "env:none", label: "Use no environment", section: SECTIONS.environments, run: () => s.setActiveEnvironment(null) });
  const runs = [...(s.history || [])].reverse().filter((h) => h?.isCollectionRun || h?.url).slice(0, 30);
  for (const h of runs) {
    const what = h.isCollectionRun ? `${h.collectionName} → ${h.requestName}` : `${h.method} ${h.url}`;
    out.push({ id: `run:${h.id}`, label: what, section: SECTIONS.runs, hint: h.statusCode != null ? String(h.statusCode) : "", run: () => s.openHistoryItem(h) });
  }
  return out;
}
