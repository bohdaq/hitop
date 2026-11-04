// The portal's home is cloud.hitop.rws8.tech: the session lives in that origin's storage, so the old
// address (hitop.rws8.tech/cloud/, still a Stripe return URL on older settings) goes there, path kept.
if (location.hostname === "hitop.rws8.tech" && location.pathname.startsWith("/cloud")) {
  location.replace(`https://cloud.hitop.rws8.tech${location.pathname.replace(/^\/cloud\/?/, "/")}${location.search}${location.hash}`);
}

import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "@lit-material/tokens/css/index.css";
// The hitop.rws8.tech theme (site/src/theme.css), shared with the site and the web app.
import "../../../site/src/theme.css";
import "./portal.css";

// Links from older servers and mails use #/ routes: mapped to paths before the router reads the address.
import "./legacy.js";
import "./portal-app.js";
