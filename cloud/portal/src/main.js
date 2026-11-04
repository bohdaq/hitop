// The portal's home is cloud.hitop.rws8.tech: the session lives in that origin's storage, so the old
// address (hitop.rws8.tech/cloud/, still a Stripe return URL on older settings) goes there.
if (location.hostname === "hitop.rws8.tech" && location.pathname.startsWith("/cloud")) {
  location.replace(`https://cloud.hitop.rws8.tech/${location.search}${location.hash}`);
}

import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "@lit-material/tokens/css/index.css";
// The hitop.rws8.tech theme (site/src/theme.css), shared with the site and the web app.
import "../../../site/src/theme.css";
import "./portal.css";

import "@lit-material/button";
import "@lit-material/text-field";

import "./portal-app.js";
