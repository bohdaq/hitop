// /404.html: served for any path that doesn't exist.
import { html } from "lit";
import { unsafeHTML } from "lit/directives/unsafe-html.js";
import { mark } from "../brand/mark.js";
import { icon } from "../icons.js";
import { footer, header } from "../shell.js";

export const meta = {
  path: "/404.html",
  title: "Page not found · HITOP",
  description: "This page does not exist on hitop.rws8.tech.",
  noindex: true,
};

export function render() {
  return html`
    ${header("/404")}
    <main id="main" class="container not-found">
      <span class="not-found-mark">${unsafeHTML(mark({ title: "HITOP" }))}</span>
      <p class="status-pill">404 Not Found</p>
      <h1>That request came back empty</h1>
      <p class="lead">Nothing lives at this address. The app is at /app/ and the guides are under /guides/.</p>
      <div class="install">
        <a class="button-filled" href="/app/">${unsafeHTML(icon("browser"))} Open the web app</a>
        <a class="text-link" href="/">Back to the home page</a>
      </div>
    </main>
    ${footer()}`;
}
