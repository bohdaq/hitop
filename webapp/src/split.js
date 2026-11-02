// The request view's split between the request editor and the response, per
// viewer (localStorage `hitop_layout`, a key nothing else reads):
//   columns            the editor's share of the width, in % (side by side)
//   rows               the editor's height in px (one above the other)
//   responseCollapsed  the response pane is folded away
// Which layout applies follows the view's width (layoutFor).
const KEY = "hitop_layout";

export const SPLIT = {
  columns: { min: 25, max: 75, initial: 50, step: 5 },
  rows: { min: 160, max: 900, initial: 280, step: 40 },
};
export const DEFAULT_LAYOUT = { columns: SPLIT.columns.initial, rows: SPLIT.rows.initial, responseCollapsed: false };

// Side by side from this width of the request view, stacked with a handle
// from ROWS_FROM, plain stacked (phones) below that.
export const COLUMNS_FROM = 880;
export const ROWS_FROM = 600;
export const layoutFor = (width) => (width >= COLUMNS_FROM ? "columns" : width >= ROWS_FROM ? "rows" : "stack");

export const clampSplit = (kind, value) => {
  const { min, max } = SPLIT[kind];
  return Math.round(Math.min(max, Math.max(min, Number.isFinite(value) ? value : SPLIT[kind].initial)));
};

// The split for a pointer at `pos` (clientX or clientY) over a box starting
// at `start` and `size` long: % of the width for columns, px for rows.
export const splitAt = (kind, pos, start, size) => clampSplit(kind, kind === "columns" ? ((pos - start) / size) * 100 : pos - start);

// The split after a key on the handle, or null when the key does nothing.
// Arrows step (Shift: four steps), Home and End go to the ends, Enter resets.
export function splitForKey(kind, value, key, shift = false) {
  const { min, max, initial, step } = SPLIT[kind];
  const by = step * (shift ? 4 : 1);
  const less = kind === "columns" ? "ArrowLeft" : "ArrowUp";
  const more = kind === "columns" ? "ArrowRight" : "ArrowDown";
  if (key === less) return clampSplit(kind, value - by);
  if (key === more) return clampSplit(kind, value + by);
  if (key === "Home") return min;
  if (key === "End") return max;
  if (key === "Enter") return initial;
  return null;
}

export function loadLayout() {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) || "{}");
    if (!v || typeof v !== "object") return { ...DEFAULT_LAYOUT };
    return { columns: clampSplit("columns", v.columns), rows: clampSplit("rows", v.rows), responseCollapsed: v.responseCollapsed === true };
  } catch {
    return { ...DEFAULT_LAYOUT };
  }
}

export function saveLayout(layout) {
  try {
    localStorage.setItem(KEY, JSON.stringify(layout));
  } catch {
    // storage full or blocked: the layout lasts for this page
  }
}
