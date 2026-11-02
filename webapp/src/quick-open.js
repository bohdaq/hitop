// Quick open (Mod+P): every saved request in every collection, and the open
// tabs that aren't saved requests, found by a fuzzy match. Pure, so it is
// unit-checked (tests/unit-ux.mjs).

const BOUNDARY = /[\s/._:\-?&=>›]/;

// How well `query` matches `text` as a subsequence, ignoring case and
// spaces in the query: null when it doesn't, else { score, at } (the
// matched character positions). Consecutive characters and characters at
// the start of a word score more; a match that starts late scores less.
export function fuzzyMatch(query, text) {
  const q = String(query).toLowerCase().replace(/\s+/g, "");
  if (!q) return { score: 0, at: [] };
  const t = String(text);
  const lower = t.toLowerCase();
  // Each start of the first character is tried; the best one wins.
  let best = null;
  for (let start = lower.indexOf(q[0]); start !== -1; start = lower.indexOf(q[0], start + 1)) {
    const at = [start];
    let score = 0;
    let i = start;
    for (let k = 1; k < q.length; k++) {
      i = lower.indexOf(q[k], i + 1);
      if (i === -1) break;
      at.push(i);
    }
    if (at.length < q.length) break; // later starts can't match either
    for (let k = 0; k < at.length; k++) {
      const p = at[k];
      score += 1;
      if (k > 0 && at[k - 1] === p - 1) score += 5;
      if (p === 0 || BOUNDARY.test(t[p - 1]) || (/[a-z]/.test(t[p - 1]) && /[A-Z]/.test(t[p]))) score += 8;
      if (k > 0) score -= Math.min(3, p - at[k - 1] - 1) * 0.5;
    }
    score -= start * 0.05;
    if (!best || score > best.score) best = { score, at };
  }
  return best;
}

// What quick open can pick from: [{ key, kind: "request" | "tab", title,
// detail, method, url, collectionId?, request?, tabIndex?, open }].
export function quickOpenItems({ collections, tabs, methodLabel }) {
  const items = [];
  tabs.forEach((t, i) => {
    if (t.loadedRequestId != null && collections.some((c) => c.id === t.loadedCollectionId && (c.requests || []).some((r) => r.id === t.loadedRequestId))) return;
    const method = t.kind === "websocket" ? "WS" : t.kind === "grpc" ? "GRPC" : t.method || "GET";
    items.push({ key: `tab:${t.id ?? i}`, kind: "tab", title: t.title || "New request", detail: "Open tab, not saved", method, url: t.url || "", tabIndex: i, open: true });
  });
  for (const c of collections) {
    for (const r of c.requests || []) {
      const folder = typeof r.file === "string" && r.file.includes("/") ? r.file.slice(0, r.file.lastIndexOf("/")).replace(/\//g, " › ") : "";
      const open = tabs.some((t) => t.loadedRequestId === r.id && t.loadedCollectionId === c.id);
      items.push({ key: `req:${c.id}:${r.id}`, kind: "request", title: r.name || "Untitled", detail: folder ? `${c.name} › ${folder}` : c.name, method: methodLabel(r), url: r.url || "", collectionId: c.id, request: r, open });
    }
  }
  return items;
}

// The items for `query`, best first (at most `limit`); each with `match`
// (positions in its title, for highlighting). An empty query keeps the
// order: open tabs first, then collections in order.
export function rankItems(items, query, limit = 50) {
  if (!String(query).trim()) return items.slice(0, limit).map((item) => ({ ...item, match: [] }));
  const out = [];
  for (const item of items) {
    const inTitle = fuzzyMatch(query, item.title);
    const inAll = fuzzyMatch(query, `${item.title} ${item.method} ${item.detail} ${item.url}`);
    if (!inTitle && !inAll) continue;
    // A match in the name counts double.
    const score = Math.max(inTitle ? inTitle.score * 2 : -Infinity, inAll ? inAll.score : -Infinity) + (item.open ? 1 : 0);
    out.push({ ...item, match: inTitle?.at ?? [], score });
  }
  return out.sort((a, b) => b.score - a.score || a.title.length - b.title.length).slice(0, limit);
}
