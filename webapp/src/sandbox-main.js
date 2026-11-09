// The extensions' script runner (sandbox.html; HITOP Next A8): answers the app page's pre/post
// calls with script-run.js, the same code the web build runs in its page. It only answers its
// parent window, and only with plain data.
import pmShimText from "../../shared/pm-shim.js?raw";
import { loadShim } from "./pm.js";
import { runPost, runPre } from "./script-run.js";

const pmShim = loadShim(pmShimText);

window.addEventListener("message", (e) => {
  if (e.source !== window.parent) return;
  const { id, kind, args } = e.data ?? {};
  if (typeof id !== "number") return;
  let reply;
  try {
    if (kind === "pre") reply = { id, result: runPre(pmShim, args) };
    else if (kind === "post") reply = { id, result: runPost(pmShim, args) };
    else reply = { id, error: `unknown call ${kind}` };
  } catch (err) {
    reply = { id, error: err?.message ?? String(err) };
  }
  // The parent is an extension page; this sandbox has an opaque origin, so "*" is the target.
  window.parent.postMessage(JSON.parse(JSON.stringify(reply)), "*");
});

window.parent.postMessage({ type: "ready" }, "*");
