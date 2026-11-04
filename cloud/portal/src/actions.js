// What every page does with a request: busy while it runs, the refusal shown next to the thing
// that failed (a 402 with its "See plans" link), success as a snackbar, and a session that ended
// elsewhere (401) back to sign-in.
import "@lit-material/alert";
import "@lit-material/button";
import { navigate } from "@lit-material/router";
import { html, nothing } from "lit";
import { ApiError } from "./api.js";
import { notify } from "./notify.js";
import { session } from "./session.js";

/** `{ message, planUrl? }` for an error; a plan limit links to the organization's plan page. */
export function describeError(e) {
  if (e instanceof ApiError && e.status === 402 && e.body?.error === "limit") {
    let planUrl = e.body.org ? `/orgs/${encodeURIComponent(e.body.org)}/plan` : "";
    if (!planUrl && e.body.upgradeUrl) {
      try {
        const u = new URL(e.body.upgradeUrl);
        planUrl = u.hash.startsWith("#/") ? u.hash.slice(1) : u.pathname;
      } catch {
        planUrl = "";
      }
    }
    return { message: e.message, planUrl };
  }
  return { message: e?.message || String(e) };
}

/** True (and the visitor is on the way to sign-in) when `e` means the session ended. */
export function sessionEnded(e) {
  if (e instanceof ApiError && e.status === 401 && session.signedIn) {
    session.expire();
    notify("Your session ended. Sign in again.");
    navigate("/sign-in", { replace: true });
    return true;
  }
  return false;
}

/**
 * Runs `fn` for a page: `host.busy` while it runs, `host.error` on a refusal, `success` (a string
 * or a function of the result) as a snackbar. Resolves `{ ok, value }`.
 */
export async function act(host, fn, { success } = {}) {
  host.error = null;
  host.busy = true;
  try {
    const value = await fn();
    if (success) notify(typeof success === "function" ? success(value) : success);
    return { ok: true, value };
  } catch (e) {
    if (!sessionEnded(e)) host.error = describeError(e);
    return { ok: false, error: e };
  } finally {
    host.busy = false;
  }
}

/** The refusal, where it happened. */
export function errorAlert(error) {
  if (!error) return nothing;
  return html`<lit-material-alert variant="error" class="refusal">
    ${error.message}
    ${error.planUrl ? html`<lit-material-button slot="action" variant="text" href=${error.planUrl}>See plans</lit-material-button>` : nothing}
  </lit-material-alert>`;
}

/** For a TaskController's `error` renderer: a 401 goes back to sign-in, anything else is shown. */
export function loadError(e) {
  if (sessionEnded(e)) return nothing;
  return errorAlert(describeError(e));
}
