// Must load before Lit, so server-rendered shadow roots are hydrated, not re-rendered.
import "@lit-labs/ssr-client/lit-element-hydrate-support.js";

import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "@lit-material/tokens/css/index.css";
import "../theme.css";
import "../site.css";
import "../landing.css";
import "../hitop.css";

import "../components.js";
import { initMenu } from "@lit-material/site-kit/client/menu.js";
import { initInstall } from "@lit-material/site-kit/client/install.js";
import { initTerminal } from "@lit-material/site-kit/client/terminal.js";
import { initReveal } from "@lit-material/site-kit/client/reveal.js";
import { initTagline } from "./tagline.js";
import { initDownloads } from "./downloads.js";

initMenu();
initInstall();
// Keep the terminal its finished height, so it doesn't shrink while it retypes.
for (const pre of document.querySelectorAll("[data-terminal]")) pre.style.minHeight = `${pre.offsetHeight}px`;
initTerminal();
initReveal();
initTagline();
initDownloads();
