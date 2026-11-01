// /privacy.html: the App Store listings link here (through the old GitHub
// Pages address, which redirects), so the path must not change.
import { html } from "lit";
import { footer, GITHUB, header } from "../shell.js";

export const meta = {
  path: "/privacy.html",
  title: "Privacy policy · HITOP",
  description:
    "HITOP has no accounts or tracking. Collections and history stay on your device. How the web app's request proxy and the usage ping you can turn on work.",
};

export function render() {
  return html`
    ${header(meta.path)}
    <main id="main" class="container page-text">
      <h1>Privacy policy</h1>
      <p class="lead">HITOP does not collect, sell or share personal data, and you never need an account. Last updated 1 November 2026.</p>

      <h2>What stays on your device</h2>
      <p>Everything you enter, including collections, requests, headers, variables, scripts and request history, is stored only on your device:</p>
      <ul>
        <li>macOS app and CLI on macOS: <code>~/Library/Application Support/hitop/</code></li>
        <li>CLI on Linux: <code>~/.config/hitop/</code></li>
        <li>CLI on Windows: <code>%APPDATA%\\hitop\\</code></li>
        <li>iOS app: the app's own storage on your iPhone or iPad</li>
        <li>Web app and browser extensions: your browser's local storage</li>
      </ul>
      <p>None of it leaves your device unless you export it yourself.</p>

      <h2>Requests you send</h2>
      <p>The Mac app, the iOS app, the CLI and the browser extensions send each request straight from your device to the address you entered. HITOP's servers are not involved.</p>
      <p>The web app at hitop.rws8.tech sends requests to public addresses through the HITOP request proxy on the same server, because browsers block most cross origin API calls. For those requests:</p>
      <ul>
        <li>The proxy receives the method, URL, headers and body you send, makes the request, and returns the response to your browser.</li>
        <li>It does not store requests, responses, headers or bodies, and does not log them. The web server's access log records your IP address, the time and the path <code>/api/proxy</code>, not where the request went.</li>
        <li>It refuses private and internal addresses. Requests to localhost and other private addresses are never sent to the proxy; your browser sends them itself.</li>
        <li>Don't send secrets you are not comfortable passing through a server you don't run. For those, use the Mac or iOS app, the CLI or an extension.</li>
      </ul>

      <h2>No tracking</h2>
      <p>HITOP contains no analytics SDKs, crash reporting, advertising identifiers, third party SDKs, session recording or fingerprinting. The website sets no cookies. If you choose a light or dark theme, your browser keeps that choice in local storage under <code>hitop-theme</code>.</p>

      <h2>Usage ping, only if you turn it on</h2>
      <p>The CLI (from version 0.3.0) and the free desktop builds from GitHub can send one anonymous ping a week, only if you turn it on. It is off by default; the desktop app asks once, with No as the default answer, and the CLI only sends it after <code>hitop telemetry on</code>. The Mac App Store and iOS apps, the web app and the browser extensions don't contain this code at all.</p>
      <ul>
        <li>The ping holds a random install id made on your device, which surface it is (CLI or desktop), the version and the operating system family. Nothing else: no requests, URLs, collections or account.</li>
        <li>It goes to <code>https://hitop.rws8.tech/api/ping</code>. The server keeps only daily counts. It uses the install id to count each install once a week, through a hash with a salt that is thrown away every week, so installs can't be followed from week to week. It doesn't store your IP address; the web server's access log records it with the time and the path, as for any page.</li>
        <li>Turn it off at any time with <code>hitop telemetry off</code> or in the desktop app's settings. <code>hitop telemetry status</code> shows what would be sent.</li>
      </ul>

      <h2>Children</h2>
      <p>HITOP does not knowingly collect information from anyone, including children under 13.</p>

      <h2>Changes</h2>
      <p>If this policy changes, the new version is posted at this address with a new date.</p>

      <h2>Contact</h2>
      <p>Email <a href="mailto:bohdaq@gmail.com">bohdaq@gmail.com</a> or open an issue on <a href=${GITHUB}>GitHub</a>.</p>
    </main>
    ${footer()}`;
}
