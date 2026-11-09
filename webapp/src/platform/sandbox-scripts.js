// Scripts in the browser extensions (HITOP Next A8). An extension page may not evaluate code
// (Manifest V3 CSP), so the scripts run in the extension's sandboxed page (sandbox.html, declared
// under "sandbox" in the manifest), which may, and which has no extension APIs and an opaque
// origin. The page talks to it by postMessage: plain data both ways, the same shapes the desktop's
// Rust scripts answer (platform/tauri.js `scripts`).

let frame = null;
let ready = null;
let next = 1;
const pending = new Map();

function start() {
  if (ready) return ready;
  ready = new Promise((resolve, reject) => {
    frame = document.createElement("iframe");
    frame.src = "./sandbox.html";
    frame.hidden = true;
    frame.setAttribute("aria-hidden", "true");
    frame.title = "Script runner";
    // Firefox has no sandboxed extension pages: there the runner never starts, and every script
    // call fails at once after this with the reason (the web app, desktop and CLI run them).
    const timer = setTimeout(() => reject(new Error("Scripts don't run in this browser's extension; the web app, the desktop app and the CLI run them")), 3000);
    window.addEventListener("message", (e) => {
      if (e.source !== frame.contentWindow) return;
      const m = e.data;
      if (m?.type === "ready") {
        clearTimeout(timer);
        resolve();
        return;
      }
      const call = pending.get(m?.id);
      if (!call) return;
      pending.delete(m.id);
      if (m.error) call.reject(new Error(m.error));
      else call.resolve(m.result);
    });
    document.body.append(frame);
  });
  return ready;
}

async function call(kind, args) {
  await start();
  const id = next++;
  return new Promise((resolve, reject) => {
    pending.set(id, { resolve, reject });
    // The sandbox has an opaque origin: "*" is the only target that reaches it; what goes out is
    // the script and its inputs, which the page would have run itself.
    frame.contentWindow.postMessage({ id, kind, args: JSON.parse(JSON.stringify(args)) }, "*");
  });
}

export const sandboxScripts = {
  pre: (args) => call("pre", args),
  // `error` (a script that threw) comes back with what it set before; the store throws it after.
  post: (args) => call("post", args),
};
