// /pricing.html: the same URL as the old site, so existing links keep working. The apps and the
// CLI stay free (or paid once); HITOP Cloud (spec/CLOUD_SERVICE.md section 3 and 8) is the only
// subscription. Prices are the owner's (Q4), set in Stripe; Stripe's checkout shows the final ones.
import { html } from "lit";
import { unsafeHTML } from "lit/directives/unsafe-html.js";
import { icon } from "../icons.js";
import { platformCards } from "../platforms.js";
import { footer, header } from "../shell.js";

export const meta = {
  path: "/pricing.html",
  title: "HITOP pricing: free apps and CLI, HITOP Cloud from $6 a month",
  description:
    "The HITOP web app, extensions, CLI and self hosted sync server are free. The Mac app is $4.99 and the iOS app $2.99, paid once. HITOP Cloud, the hosted end to end encrypted team service, is free for 3 people, Pro $6 a month, Team $10 per seat.",
};

export const PORTAL = "https://cloud.hitop.rws8.tech/";

const FREE_FOREVER = [
  "The web app, the Chrome and Firefox extensions and the CLI, MIT licensed",
  "Collections as YAML files in your repo, reviewed in pull requests",
  "hitop run in CI, the GitHub Action, scheduled runs and the MCP server",
  "The whole sync server to run yourself (AGPL): teams, roles, SSO, audit log, no limits and no license keys",
  "No account needed for any of it",
];

const TIERS = [
  {
    name: "Free",
    price: "$0",
    note: "Try sync with a friend",
    points: ["3 people, 3 shared workspaces", "100 MB of encrypted storage", "3 devices per person", "5 runner credentials for your CI"],
    action: [PORTAL, "Sign up free"],
  },
  {
    name: "Pro",
    price: "$6",
    note: "A month, billed yearly ($8 monthly), plus applicable tax. One person",
    points: ["You plus 2 guests, 25 workspaces", "2 GB, 10 devices", "10 runner credentials", "Run history and webhook alerts from your runners"],
    action: [PORTAL, "Get Pro"],
  },
  {
    name: "Team",
    price: "$10",
    note: "Per seat a month, billed yearly ($12 monthly), plus applicable tax",
    points: ["Unlimited workspaces, roles and invites", "10 GB plus 1 GB per seat", "5 guest viewers free", "Run history, Slack alerts, shared secrets", "14 day free trial, no card"],
    action: [PORTAL, "Start the Team trial"],
  },
  {
    name: "Business and Enterprise",
    price: "$20",
    note: "Per seat a month for Business, plus applicable tax. Enterprise by quote",
    points: ["Your own SSO provider for your verified domains", "Signed audit export, daily anchors to the owners", "Admin console across every workspace", "A year of run history, priority support", "Enterprise: self hosted with a support contract"],
    action: ["mailto:bohdaq@gmail.com?subject=HITOP%20Cloud%20Business", "Talk to us"],
  },
];

const FAQ = [
  ["Is there a subscription?", "Only for HITOP Cloud, the hosted team service. The web app, the extensions, the CLI and the self hosted sync server stay free, and the Mac and iOS apps are paid once with free updates."],
  ["Can HITOP Cloud read my collections?", "No. Collections are encrypted on your devices before they are sent, and the server never holds a key that opens them. The security page says what the server does see, and that no outside firm has reviewed the encryption yet."],
  ["Do you run monitors for me?", "No. Run your collections in your CI or with hitop schedule; HITOP Cloud keeps the encrypted run history and sends alerts when a run starts or stops failing. Running your collection on our machines would need a key we are built never to hold."],
  ["What counts as a seat?", "Every person in the organization or in one of its workspaces, viewers included. Team includes 5 guest viewers for free, so you can show a collection to a stakeholder. Runner credentials never count."],
  ["What happens if I go over a limit or stop paying?", "New workspaces, people or uploads beyond the plan are refused with a note that names the limit. Nothing is deleted or locked: you can always read, pull and export. After a failed payment Stripe retries it for a while (the retry schedule is set in Stripe) before the plan drops to Free limits."],
  ["Who handles payment and tax?", "Stripe is the merchant of record (Stripe Managed Payments; the checkout says \"Sold through Link\"): it takes the payment, adds VAT or sales tax where it applies, and sends the receipts and invoices. HITOP never sees your card."],
  ["Can I get a refund?", html`<span id="refunds">HITOP Cloud: a full refund within 14 days of the first payment, by mail to <a href="mailto:bohdaq@gmail.com">bohdaq@gmail.com</a>. App Store purchases are refunded by Apple at reportaproblem.apple.com.</span>`],
  ["Can I buy HITOP Cloud in the Mac or iOS app?", "No. You can sign in to sync there; plans are bought on the web."],
  ["Why pay for the native apps?", "They send requests from the operating system, so they reach localhost with no CORS setup and machines on your network as well as public APIs, and they keep working when you are offline."],
];

function tierCards() {
  return html`
    <div class="plans">
      ${TIERS.map(
        (t) => html`
          <article class="plan">
            <span class="tile-icon">${unsafeHTML(icon(t.name === "Free" ? "user" : t.name === "Pro" ? "user-circle" : t.name === "Team" ? "users-three" : "buildings"))}</span>
            <h3>${t.name}</h3>
            <p class="plan-price">${t.price}</p>
            <p class="plan-note">${t.note}</p>
            <ul>
              ${t.points.map((point) => html`<li>${unsafeHTML(icon("check"))}<span>${point}</span></li>`)}
            </ul>
            <a class="text-link" href=${t.action[0]}>${t.action[1]} ${unsafeHTML(icon("arrow-right"))}</a>
          </article>`,
      )}
    </div>
    <p class="section-lead small">Prices in US dollars, plus applicable tax; Stripe's checkout may show your currency. Run history and alerts arrive with the paid launch.</p>`;
}

export function render() {
  return html`
    ${header(meta.path)}
    <main id="main">
      <section class="container page-hero">
        <h1 class="hero-heading">Free on your machines, <br />paid only where we host</h1>
        <p class="lead">Everything that runs on your devices or your own server is free. HITOP Cloud hosts your team's workspaces end to end encrypted, for a monthly price.</p>
      </section>

      <section class="section container reveal" aria-labelledby="free-title">
        <h2 id="free-title" class="section-title">Free forever</h2>
        <ul class="proof-list">
          ${FREE_FOREVER.map((item) => html`<li>${unsafeHTML(icon("check-circle"))}<span>${item}</span></li>`)}
        </ul>
      </section>

      <section class="section container reveal" aria-labelledby="cloud-title">
        <h2 id="cloud-title" class="section-title">HITOP Cloud</h2>
        <p class="section-lead small">Hosted sync for people and teams. Your collections are encrypted before they leave your device; the server stores ciphertext.</p>
        ${tierCards()}
      </section>

      <section class="section container reveal" aria-labelledby="plans-title">
        <h2 id="plans-title" class="section-title">The apps</h2>
        ${platformCards()}
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
        <p class="proof-line">No account needed. HITOP Cloud is optional.</p>
      </section>
    </main>
    ${footer()}`;
}
