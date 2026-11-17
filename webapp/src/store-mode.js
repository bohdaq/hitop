// ST1 free store apps: the desktop's store builds (Mac App Store, Microsoft Store; Rust without the
// Cargo feature `github`, platform().storeBuild) show no purchase call to action (Apple 3.1.1/3.1.3,
// owner 2026-11-17: "No purchase in the app"): no prices, no Pay/Choose/Upgrade/trial/seat buttons,
// no link to pricing, checkout, billing or the portal's plan page, and no text sending people to
// buy elsewhere. The plan's name, usage meters and neutral limit messages stay. The web app and
// the GitHub Release builds keep everything.
import { platform } from "./platform/index.js";

export function storeBuild() {
  try {
    return platform().storeBuild === true;
  } catch {
    return false;
  }
}

/** The Organization area's plan section, the account menu item and the buttons that open it. */
export const planLabel = () => (storeBuild() ? "Plan & usage" : "Plan & billing");
