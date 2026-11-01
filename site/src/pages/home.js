// / — the landing page. One action: open the free web app.
import { html } from "lit";
import { unsafeHTML } from "lit/directives/unsafe-html.js";
import { icon } from "../icons.js";
import { platformCards } from "../platforms.js";
import { CRATE, footer, GITHUB, header, IOS_APP, MAC_APP } from "../shell.js";

export const meta = {
  path: "/",
  title: "HITOP: a free API client with no account and no CORS errors",
  description:
    "Send HTTP requests, save them in collections, script them with JavaScript and run them in order. Free in the browser and on the command line, native on Mac and iPhone. No account, MIT licensed.",
};

const TILES = [
  ["arrows-left-right", "No CORS errors in the browser", "The web app sends requests to public APIs through its own proxy, so an API that never set CORS headers still answers. localhost goes direct."],
  ["folders", "Collections you can run", "Group requests by API, reorder them by dragging, and run the whole collection in order to check a flow end to end."],
  ["brackets-curly", "Variables in every field", "Write ${baseUrl} or ${token} once per collection and use it in URLs, headers and bodies."],
  ["code", "Scripts before and after", "JavaScript runs before a request to set a signature or timestamp, and after it to pull a token out of the response for the next one."],
  ["clock-counter-clockwise", "History with timings", "The last 50 requests with status codes and times. Open one again with a click."],
  ["plugs-connected", "HTTP, GraphQL, WebSocket, gRPC", "GraphQL with schema completion, WebSocket connections with saved messages (the web app and CLI), and gRPC calls from the CLI by server reflection or .proto files."],
  ["file-code", "From OpenAPI or Postman", "Turn an OpenAPI 3.0 or 3.1 spec or a Postman export into a collection, then sync the next spec version without losing your edits."],
  ["git-diff", "Code snippets and diffs", "Copy a request as curl, fetch, Python, Go, Java or Rust, and compare a response with a saved example or an earlier one."],
  ["shield-check", "Nothing to sign up for", "No account, no workspace sync, no analytics. Collections live on your device and move as a JSON file, or as Postman v2.1."],
];

const STEPS = [
  ["Open the web app", html`No install and no sign up. Or get the <a href=${MAC_APP}>Mac</a> or <a href=${IOS_APP}>iOS</a> app.`],
  ["Send a request", html`Pick a method, paste a URL, add headers and a body. The response comes back highlighted, with headers and timing.`],
  ["Save and run", html`Save it to a collection, add <code>\${variables}</code> and scripts, then run the collection in order.`],
];

const FAQ = [
  ["Why do API calls fail in other browser tools?", "Browsers block a page from reading another site's response unless that site sends CORS headers, and most APIs don't. The HITOP web app avoids this by sending requests to public addresses through its proxy at hitop.rws8.tech."],
  ["Does the proxy keep my requests?", "No. It makes the request and returns the response, without storing or logging headers or bodies. The privacy policy has the details. For secrets you'd rather not pass through any server, use the Mac or iOS app, the CLI or an extension: they send requests from your device."],
  ["Can I test localhost?", "Yes. The web app sends requests to localhost straight from your browser, so your local API must allow the origin https://hitop.rws8.tech in its CORS headers. For other machines on your network, use the Mac app, the CLI or an extension: browsers block a public page from calling them."],
  ["Is it really free?", "The web app, the Chrome and Firefox extensions and the CLI are free and MIT licensed. The Mac app is $4.99 and the iOS app is $2.99, each paid once."],
  ["Can I bring my Postman collections?", "Yes. Import Postman v2.1 collections, and export back to Postman v2.1 or HITOP JSON."],
  ["Where is my data stored?", "On your device: browser storage for the web app and extensions, files for the Mac app and CLI, app storage on iOS. Nothing is synced to a server."],
  ["Does it work offline?", "The Mac app, the iOS app and the CLI work with no internet connection, which is handy for local and air gapped services."],
  ["Can I run collections in CI?", html`Yes, with the CLI: <code>hitop run ./my-api --env ci --reporter junit</code> runs a collection folder from your repository with its scripts and variables and writes a JUnit report (or JSON, or one HTML page). <code>hitop schedule</code> writes a scheduled workflow for you. On GitHub there is an <a href="/terms/run-hitop-in-github-actions.html">action</a>, and a Docker image for other pipelines.`],
];

const TAGLINE = ["Your requests, keys and responses stay on your device.", "No account, no workspace sync, no tracking."];

// A request as the app shows it: the hero's picture of the product.
const RESPONSE = `{
  "id": 4815,
  "name": "Dana Whitfield",
  "email": "dana@northwind.dev",
  "plan": "team",
  "created_at": "2026-10-28T09:14:52Z"
}`;

const CLI = [
  { cmd: "cargo install hitop" },
  { cmd: "hitop variables set Billing baseUrl https://api.northwind.dev" },
  { out: "✓ Set baseUrl=https://api.northwind.dev" },
  { cmd: "hitop run Billing" },
  { out: "▶ Billing" },
  { out: "Summary: 3/3 requests succeeded", accent: true },
];

function appWindow() {
  return html`
    <figure class="app-window" aria-label="The HITOP app: a GET request to api.northwind.dev/v1/customers/4815 returned 200 OK in 212 ms with a JSON customer record.">
      <div class="app-bar" aria-hidden="true"><span></span><span></span><span></span><b>HITOP</b></div>
      <div class="app-body" aria-hidden="true">
        <div class="app-request">
          <span class="method">GET</span>
          <span class="url">\${baseUrl}/v1/customers/4815</span>
          <span class="send">Send</span>
        </div>
        <div class="app-headers">
          <span>Authorization</span><span>Bearer \${token}</span>
          <span>Accept</span><span>application/json</span>
        </div>
        <div class="app-status"><span class="status-pill">200 OK</span><span>212 ms</span><span>148 B</span></div>
        <pre class="app-response">${RESPONSE}</pre>
      </div>
    </figure>`;
}

function terminal() {
  return html`
    <div class="terminal" role="img" aria-label="A terminal: installing the HITOP CLI with cargo, setting a collection variable and running the collection; 3 of 3 requests succeed.">
      <div class="terminal-bar" aria-hidden="true"><span></span><span></span><span></span></div>
      <pre aria-hidden="true" data-terminal=${JSON.stringify(CLI)}>${CLI.map((l) =>
        l.cmd ? html`<span class="t-line"><span class="t-prompt">$ </span>${l.cmd}</span>` : html`<span class=${l.accent ? "t-line t-accent" : "t-line t-out"}>${l.out}</span>`,
      )}</pre>
    </div>`;
}

export function render() {
  return html`
    ${header(meta.path)}
    <main id="main">
      <section class="hero container">
        <div class="hero-text">
          <h1 class="hero-heading">Test HTTP APIs <br />without an account <br />or a CORS error</h1>
          <p class="lead">HITOP is a free, open source API client. Send requests, keep them in collections, script them in JavaScript and run them in order, with everything stored on your device.</p>
          <div class="install">
            <a class="button-filled" href="/app/">${unsafeHTML(icon("browser"))} Open the web app</a>
            <a class="text-link" href="/pricing.html">Mac, iOS and CLI ${unsafeHTML(icon("arrow-right"))}</a>
          </div>
          <p class="proof-line">MIT licensed · On the Mac App Store and the App Store · No sign up</p>
        </div>
        ${appWindow()}
      </section>

      <section id="features" class="section container reveal" aria-labelledby="features-title">
        <h2 id="features-title" class="section-title">What you get, free</h2>
        <div class="tiles">
          ${TILES.map(
            ([name, title, body]) => html`
              <div class="tile static">
                <span class="tile-icon">${unsafeHTML(icon(name))}</span>
                <h3>${title}</h3>
                <p>${body}</p>
              </div>`,
          )}
        </div>
      </section>

      <section class="tagline container" aria-label="What HITOP promises">
        <p class="tagline-text">${TAGLINE.map(
          (line) => html`<span class="tagline-line">${line.split(" ").map((w) => html`<span class="word">${w}</span> `)}</span>`,
        )}</p>
      </section>

      <section class="section container reveal" aria-labelledby="how-title">
        <h2 id="how-title" class="section-title">From URL to collection in a minute</h2>
        <ol class="steps">
          ${STEPS.map(([title, body], i) => html`<li><span class="step-number">${i + 1}</span><h3>${title}</h3><p>${body}</p></li>`)}
        </ol>
      </section>

      <section class="section container reveal" aria-labelledby="platforms-title">
        <h2 id="platforms-title" class="section-title">One client on every device you test from</h2>
        <p class="section-lead">The same collections, variables and scripts in each. Move a collection between them as a JSON file.</p>
        ${platformCards()}
      </section>

      <section class="section container reveal library" aria-labelledby="cli-title">
        <div>
          <h2 id="cli-title" class="section-title">Run the same collections in CI</h2>
          <p class="section-lead">The CLI is a single Rust binary. It runs collections with their pre and post request scripts, passes values from one request to the next, and writes JUnit and JSON reports for any pipeline.</p>
          <p class="section-lead"><code>hitop mcp</code> lets Claude Code, Cursor and other MCP clients list, run and edit the requests of a collection folder, with secrets masked.</p>
          <a class="text-link" href=${CRATE}>Install from crates.io ${unsafeHTML(icon("arrow-right"))}</a>
          <a class="text-link" href="/terms/use-hitop-with-claude-code-and-cursor.html">Use HITOP with Claude Code and Cursor ${unsafeHTML(icon("arrow-right"))}</a>
        </div>
        ${terminal()}
      </section>

      <section class="section container reveal" aria-labelledby="faq-title">
        <h2 id="faq-title" class="section-title">Questions</h2>
        <div class="faq">
          ${FAQ.map(([q, a]) => html`<details><summary>${q}${unsafeHTML(icon("plus"))}</summary><p>${a}</p></details>`)}
        </div>
      </section>

      <section class="final-cta container reveal" aria-labelledby="final-title">
        <h2 id="final-title" class="section-title">Send your first request now</h2>
        <div class="install">
          <a class="button-filled" href="/app/">${unsafeHTML(icon("browser"))} Open the web app</a>
          <a class="text-link" href=${GITHUB}>Read the source ${unsafeHTML(icon("arrow-right"))}</a>
        </div>
        <p class="proof-line">Free and MIT licensed. No account. Your data stays on your device.</p>
      </section>
    </main>
    ${footer()}`;
}

export function structuredData() {
  return [
    {
      "@context": "https://schema.org",
      "@type": "SoftwareApplication",
      name: "HITOP",
      applicationCategory: "DeveloperApplication",
      operatingSystem: "Web, macOS, iOS, Linux, Windows",
      license: "https://opensource.org/licenses/MIT",
      url: "https://hitop.rws8.tech/",
      offers: [
        { "@type": "Offer", name: "Web app, extensions and CLI", price: "0", priceCurrency: "USD" },
        { "@type": "Offer", name: "macOS app", price: "4.99", priceCurrency: "USD", url: MAC_APP },
        { "@type": "Offer", name: "iOS app", price: "2.99", priceCurrency: "USD", url: IOS_APP },
      ],
    },
    {
      "@context": "https://schema.org",
      "@type": "FAQPage",
      mainEntity: FAQ.filter(([, a]) => typeof a === "string").map(([q, a]) => ({
        "@type": "Question",
        name: q,
        acceptedAnswer: { "@type": "Answer", text: a },
      })),
    },
  ];
}
