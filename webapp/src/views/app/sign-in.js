// A part of the app shell (HitopApp, ../hitop-app.js).
import { storageOf } from "../../git.js";
import { syncClient } from "../sync-view.js";

export const SignInPart = (Base) =>
  class extends Base {
  // --- sign in ----------------------------------------------------------------------

  /** The hitop sync client of this app (views and the e2e helpers sign in through it). */
  sync() {
    return syncClient(this.store);
  }

  /** The Sign in dialog (sign-in-dialog.js). `then` runs after signing in (a Share dialog reopens). */
  openSignIn(then = null, reason = null) {
    this.signInThen = then;
    // UX2: why it is asked ("Sign in to open Plan & billing"), the dialog's headline.
    this.signInReason = reason;
    this.openDialog("sign-in", { step: "loading", title: reason || "Sign in to HITOP" });
  }

  /**
   * Just signed in (the dialog, or a test through `sync()`): the library links and pulls, the dialog
   * closes, a toast says who, focus goes to the account button. `r` = finishBrowserSignIn's answer.
   */
  async signedIn(r = {}, fromDialog = false) {
    const s = this.store;
    const sync = this.sync();
    const a = sync.account();
    const then = fromDialog ? this.signInThen : null;
    this.signInThen = null;
    this.signInReason = null;
    // With somewhere to go back to (a Share dialog), the dialog turns into it; else it closes.
    if (then) then();
    else if (this.dialog?.kind === "sign-in") this.closeDialog();
    await s.library?.checkAccount().catch(() => {});
    // The organizations: main.js refreshOrgs loads them once per account, and can run mid sign-in
    // (account saved, session not yet), fail and not retry; so load them here, signed in for sure.
    await import("../../orgs.js").then(({ loadOrgs }) => loadOrgs(s, sync)).catch(() => {});
    this.requestUpdate();
    if (!a) return;
    // How many collections now sync to HITOP Cloud: the library's own record (git.js storageOf),
    // only when the library is on and holds some.
    const n = s.library?.status.on ? s.collections.filter((c) => ["cloud", "github"].includes(storageOf(s, c).kind)).length : 0;
    let text = `Signed in as ${a.email}${n ? ` · ${n === 1 ? "1 collection now syncs" : `${n} collections now sync`}` : ""}`;
    // A new device of an account with shared collections: their keys open with the recovery
    // phrase, the password or another device (Account, Password & recovery), not with Google or GitHub.
    if (r.newDevice) {
      const shared = await sync.workspaces().catch(() => []);
      if (shared?.length) text += ". To open your shared collections here, unlock this device (Account, Password & recovery)";
    }
    // The Account page, if it is on screen under the dialog, shows the new account.
    for (const el of this.querySelectorAll("hitop-sync")) el.requestUpdate();
    this.toast(text);
    await this.updateComplete;
    if (!then) this.querySelector("header.topbar .account-button")?.focus();
  }

  // Back from "Continue in this tab": the provider sent this tab to the app with `#sso=<code>`
  // (`?sso=<code>` on a loopback origin such as http://localhost).
  resumeSignIn() {
    const m = /^#sso=(.+)$/.exec(location.hash);
    const q = new URLSearchParams(location.search);
    const code = m ? decodeURIComponent(m[1]) : q.get("sso");
    if (!code) return;
    q.delete("sso");
    const rest = q.toString();
    history.replaceState(null, "", location.pathname + (rest ? `?${rest}` : ""));
    this.openDialog("sign-in", { step: "finishing", title: "Sign in to HITOP", resume: code });
  }
  };
