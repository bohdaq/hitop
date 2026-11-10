// U6: the newcomer's three steps (the sidebar's "Get started" card), each checked off from what
// the app can see: a request sent (History), a request saved to a collection of one's own (not the
// sample), a collection run (the run history or a run on screen). Shown only to someone whose app
// was empty the first time this version saw it, until the steps are done or the card is hidden;
// remembered per browser in localStorage `hitop_getting_started` {fresh, hidden} (a new key, the
// React app ignores it). Pure but for storage, unit-checked in tests/unit-ux.mjs.

const KEY = "hitop_getting_started";
export const SAMPLE_NAME = "Sample: JSONPlaceholder";

/** The three steps, done or not. `runs`: the run history ({collectionId: [...]}) or null. */
export function gettingStartedSteps({ history = [], collections = [], runs = null, ranNow = false } = {}) {
  const sent = history.length > 0;
  const saved = collections.some((c) => c.name !== SAMPLE_NAME && (c.requests || []).some((r) => String(r.url ?? "").trim()));
  const ran = ranNow || Object.values(runs ?? {}).some((list) => Array.isArray(list) && list.length > 0);
  return [
    { id: "send", label: "Send a request", hint: "Type a URL above and press Send (⌘↵), or open the sample.", done: sent },
    { id: "save", label: "Save a request to a collection", hint: "⌘S saves the open request; pick a collection or make one.", done: saved },
    { id: "run", label: "Run a collection", hint: "On a collection's page, Run collection sends its requests in order and checks their tests.", done: ran },
  ];
}

function read(storage) {
  try {
    const v = JSON.parse(storage?.getItem(KEY) ?? "null");
    return v && typeof v === "object" ? v : null;
  } catch {
    return null;
  }
}

function write(storage, v) {
  try {
    storage?.setItem(KEY, JSON.stringify(v));
  } catch {
    // storage blocked or full: the card just doesn't remember
  }
}

/**
 * Whether the card shows. The first call decides who is a newcomer (`fresh`: the app was empty)
 * and remembers it, so someone who already had requests never sees the card.
 */
export function showGettingStarted(storage, { fresh, steps }) {
  let state = read(storage);
  if (!state) {
    state = { fresh: Boolean(fresh) };
    write(storage, state);
  }
  return Boolean(state.fresh) && !state.hidden && steps.some((s) => !s.done);
}

export function hideGettingStarted(storage) {
  write(storage, { ...(read(storage) ?? { fresh: true }), hidden: true });
}
