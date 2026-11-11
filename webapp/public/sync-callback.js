// The way back from "Continue with GitHub / Google / SSO" in a pop-up (views/sign-in-dialog.js):
// the sync server sends the browser here with `?flow=<id>#sso=<one-time code>`. The code goes to
// the app's sign-in dialog on this origin only (a BroadcastChannel, which still works when the
// provider's pages cut the pop-up off from its opener; and the opener, when there is one), and the
// window closes. The code alone signs nobody in: the dialog also holds the verifier.
(function () {
  const flow = new URLSearchParams(location.search).get("flow") || "";
  const m = /^#sso=(.+)$/.exec(location.hash);
  const code = m ? decodeURIComponent(m[1]) : "";
  history.replaceState(null, "", location.pathname + location.search);
  const status = document.getElementById("status");
  if (!code) {
    status.textContent = "This sign-in didn't finish. Close this window and try again in HITOP.";
    return;
  }
  const message = { hitopSignIn: { flow, code } };
  try {
    const channel = new BroadcastChannel("hitop-sign-in");
    channel.postMessage(message);
    channel.close();
  } catch {
    // no BroadcastChannel: the opener below
  }
  try {
    window.opener?.postMessage(message, location.origin);
  } catch {
    // cut off from the opener
  }
  status.textContent = "Signed in. You can close this window and go back to HITOP.";
  setTimeout(() => window.close(), 200);
})();
