// The document around every prerendered page: head, header, footer.
// Same structure as rws8.tech's shell, with HITOP's pages and links.
import { html } from "lit";
import { themeBootScript } from "@lit-material/site-kit/client/theme.js";
import { unsafeHTML } from "lit/directives/unsafe-html.js";
import { mark } from "./brand/mark.js";
import { icon } from "./icons.js";
import { DRAFT, LEGAL_LINKS } from "./legal-status.js";

export const ORIGIN = "https://hitop.rws8.tech";
export const GITHUB = "https://github.com/bohdaq/hitop";
export const MAC_APP = "https://apps.apple.com/app/hitop-api-client/id6785294166";
export const IOS_APP = "https://apps.apple.com/app/hitop-http-client/id6785326628";
export const CHROME = "https://chromewebstore.google.com/detail/hitop-http-api-testing-to/mmbpagencfjfgeicmeobbmeocdpeokdk";
export const FIREFOX = "https://addons.mozilla.org/addon/hitop/";
export const CRATE = "https://crates.io/crates/hitop";

const NAV = [
  ["/#features", "Features", null],
  ["/pricing.html", "Pricing", "/pricing.html"],
  ["/guides/", "Guides", "/guides/"],
  ["/app/", "Web app", "/app/"],
];

function links(path) {
  return html`${NAV.map(
    ([href, label, match]) => html`<a href=${href} aria-current=${match && path.startsWith(match) ? "page" : "false"}>${label}</a>`,
  )}<a href=${GITHUB}>GitHub</a>`;
}

export function header(path) {
  return html`
    <header class="site-header">
      <nav aria-label="Main">
        <a class="lockup" href="/" aria-label="HITOP home" aria-current=${path === "/" ? "page" : "false"}>
          ${unsafeHTML(mark({ title: "HITOP" }))}
          <span class="word"><b>HITOP</b><small>API CLIENT</small></span>
        </a>
        <span class="nav-links">${links(path)}</span>
        <button class="icon-button" type="button" data-theme-toggle aria-pressed="false" aria-label="Switch theme">
          <span class="sun">${unsafeHTML(icon("sun"))}</span>
          <span class="moon">${unsafeHTML(icon("moon"))}</span>
        </button>
        <button class="icon-button menu-toggle" type="button" data-menu-toggle aria-expanded="false" aria-controls="mobile-menu" aria-label="Open menu">
          <span class="menu-icon" aria-hidden="true"><span></span><span></span></span>
        </button>
      </nav>
      <div id="mobile-menu" class="mobile-menu" hidden>
        <nav aria-label="Main">${links(path)}</nav>
      </div>
    </header>`;
}

export function footer() {
  return html`
    <footer class="site-footer">
      <div class="container footer-grid">
        <ul>
          <li><a href="/app/">Web app</a></li>
          <li><a href=${MAC_APP}>Mac App Store</a></li>
          <li><a href=${IOS_APP}>App Store</a></li>
          <li><a href=${CHROME}>Chrome</a></li>
          <li><a href=${FIREFOX}>Firefox</a></li>
          <li><a href=${CRATE}>crates.io</a></li>
          <li><a href="/download/">Downloads</a></li>
        </ul>
        <ul>
          <li><a href="/pricing.html">Pricing</a></li>
          <li><a href="/guides/">Guides</a></li>
          <li><a href="/privacy.html">Privacy</a></li>
          ${DRAFT ? "" : LEGAL_LINKS.map(([href, label]) => html`<li><a href=${href}>${label}</a></li>`)}
          <li><a href=${`${GITHUB}/issues`}>Report a problem</a></li>
          <li><a href=${GITHUB}>GitHub</a></li>
          <li><a href=${`${GITHUB}/blob/main/LICENSE`}>MIT license</a></li>
        </ul>
        <p>© 2026 <a href="https://bohdantsap.com/" rel="author">Bohdan Tsap</a>. Served by <a href="https://rws8.tech/">rws</a>.</p>
      </div>
    </footer>`;
}

// Applies a saved theme before first paint, so a dark-mode visitor never sees
// a cream flash. Storage can throw (private windows), hence the try.
const THEME_BOOT = themeBootScript("hitop-theme");

export function documentHtml({ title, description, path, body, assets, noindex = false, canonical = path, jsonLd = [], ogType = "website", markdown = null, modified = null }) {
  const css = assets.css.map((href) => `<link rel="stylesheet" href="${href}">`).join("");
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${title}</title>
<meta name="description" content="${description}">
${noindex ? '<meta name="robots" content="noindex">' : ""}
<link rel="canonical" href="${ORIGIN}${canonical}">
<link rel="icon" href="/favicon.svg" type="image/svg+xml">
<link rel="icon" href="/favicon.ico" sizes="32x32">
${markdown ? `<link rel="alternate" type="text/markdown" href="${markdown.path}" title="This page as Markdown">` : ""}
<link rel="apple-touch-icon" href="/apple-touch-icon.png">
<meta property="og:type" content="${ogType}">
<meta property="og:site_name" content="HITOP">
${modified ? `<meta property="article:modified_time" content="${modified}">` : ""}
<meta property="og:url" content="${ORIGIN}${canonical}">
<meta property="og:title" content="${title}">
<meta property="og:description" content="${description}">
<meta property="og:image" content="${ORIGIN}/social-preview.png">
<meta name="twitter:card" content="summary_large_image">
<meta name="apple-itunes-app" content="app-id=6785326628">
<meta name="theme-color" content="#16120F">
<script>${THEME_BOOT}</script>
${jsonLd.map((d) => `<script type="application/ld+json">${JSON.stringify(d).replace(/</g, "\\u003c")}</script>`).join("")}
${css}
<script type="module" src="${assets.js}"></script>
</head>
<body>
<a class="skip-link" href="#main">Skip to content</a>
${body}
</body>
</html>
`;
}
