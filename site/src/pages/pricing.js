// /pricing.html: the same URL as the old site, so existing links keep working.
import { html } from "lit";
import { unsafeHTML } from "lit/directives/unsafe-html.js";
import { icon } from "../icons.js";
import { platformCards } from "../platforms.js";
import { footer, header } from "../shell.js";

export const meta = {
  path: "/pricing.html",
  title: "HITOP pricing: free web app and CLI, $4.99 Mac app, $2.99 iOS app",
  description:
    "HITOP is free in the browser, as a Chrome or Firefox extension and on the command line. The native Mac app is $4.99 and the iPhone and iPad app is $2.99, each a one time purchase.",
};

const INCLUDED = [
  "GET, POST, PUT, PATCH, DELETE, HEAD and OPTIONS with any headers and body",
  "Collections, run in order with one click",
  "Collection variables with ${name} in URLs, headers and bodies",
  "JavaScript before and after each request",
  "Request history with status codes and timings",
  "Highlighted JSON, XML and HTML responses",
  "Import and export as HITOP JSON or Postman v2.1",
  "No account and no sign up",
];

const FAQ = [
  ["Is there a subscription?", "No. The web app, the extensions and the CLI are free. The Mac and iOS apps are paid once, and updates are free."],
  ["Why pay for the native apps?", "They send requests from the operating system, so they reach localhost and machines on your network as well as public APIs, and they keep working when you are offline."],
  ["Do I need to buy both native apps?", "No. They are separate purchases; buy the one for the device you use. Export a collection as JSON to move it between them."],
  ["Can I get a refund?", "Refunds for App Store purchases are handled by Apple at reportaproblem.apple.com."],
];

export function render() {
  return html`
    ${header(meta.path)}
    <main id="main">
      <section class="container page-hero">
        <h1 class="hero-heading">Free where you test, <br />paid once where you live</h1>
        <p class="lead">Every version has the full feature set. You pay only for the native apps, once, with no subscription.</p>
      </section>

      <section class="section container" aria-labelledby="plans-title">
        <h2 id="plans-title" class="visually-hidden">Platforms and prices</h2>
        ${platformCards()}
      </section>

      <section class="section container reveal" aria-labelledby="included-title">
        <h2 id="included-title" class="section-title">Included everywhere</h2>
        <ul class="proof-list">
          ${INCLUDED.map((item) => html`<li>${unsafeHTML(icon("check-circle"))}<span>${item}</span></li>`)}
        </ul>
      </section>

      <section class="section container reveal" aria-labelledby="faq-title">
        <h2 id="faq-title" class="section-title">Questions about buying</h2>
        <div class="faq">
          ${FAQ.map(([q, a]) => html`<details><summary>${q}${unsafeHTML(icon("plus"))}</summary><p>${a}</p></details>`)}
        </div>
      </section>

      <section class="final-cta container reveal" aria-labelledby="final-title">
        <h2 id="final-title" class="section-title">Try it free before you buy</h2>
        <div class="install">
          <a class="button-filled" href="/app/">${unsafeHTML(icon("browser"))} Open the web app</a>
        </div>
        <p class="proof-line">No account. Your collections stay on your device.</p>
      </section>
    </main>
    ${footer()}`;
}
