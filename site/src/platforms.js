// Where HITOP runs and what each costs: the home page's platform section and
// the pricing page render the same list.
import { html } from "lit";
import { unsafeHTML } from "lit/directives/unsafe-html.js";
import { icon } from "./icons.js";
import { CHROME, CRATE, FIREFOX, IOS_APP, MAC_APP } from "./shell.js";

export const PLATFORMS = [
  {
    icon: "browser",
    name: "Web app",
    price: "Free",
    note: "Any modern browser",
    points: ["Public APIs go through the HITOP proxy, so no CORS errors", "Requests to localhost go straight from your browser", "Data stays in your browser storage"],
    action: ["/app/", "Open the web app"],
  },
  {
    icon: "desktop",
    name: "macOS",
    price: "$4.99",
    note: "One time · macOS 12 or later",
    points: ["Native app, requests sent by macOS itself", "Reaches localhost and your private network", "Collections saved as files on your Mac"],
    action: [MAC_APP, "Get the Mac app"],
  },
  {
    icon: "device-mobile",
    name: "iPhone and iPad",
    price: "$2.99",
    note: "One time · iOS 17 or later",
    points: ["Check an API from your phone in seconds", "Requests sent by iOS itself, no proxy", "Collections stored on the device"],
    action: [IOS_APP, "Get the iOS app"],
  },
  {
    icon: "terminal-window",
    name: "CLI",
    price: "Free",
    note: "Linux, macOS and Windows",
    points: ["Run whole collections in CI", "Same collections, variables and scripts", "One line: curl -fsSL https://hitop.rws8.tech/install.sh | sh"],
    action: [CRATE, "See it on crates.io"],
  },
];

export const EXTENSIONS = [
  [CHROME, "Chrome extension"],
  [FIREFOX, "Firefox extension"],
];

export function platformCards() {
  return html`
    <div class="plans">
      ${PLATFORMS.map(
        (p) => html`
          <article class="plan">
            <span class="tile-icon">${unsafeHTML(icon(p.icon))}</span>
            <h3>${p.name}</h3>
            <p class="plan-price">${p.price}</p>
            <p class="plan-note">${p.note}</p>
            <ul>
              ${p.points.map((point) => html`<li>${unsafeHTML(icon("check"))}<span>${point}</span></li>`)}
            </ul>
            <a class="text-link" href=${p.action[0]}>${p.action[1]} ${unsafeHTML(icon("arrow-right"))}</a>
          </article>`,
      )}
    </div>
    <p class="section-lead small">
      Also free as a browser extension for
      ${EXTENSIONS.map(([href, label], i) => html`${i ? " and " : ""}<a href=${href}>${label.replace(" extension", "")}</a>`)}.
    </p>`;
}
