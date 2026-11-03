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
      <p class="lead">HITOP does not collect, sell or share personal data, and you never need an account. The one exception is optional: the hitop sync test server keeps an account and a security log if you sign in to it (see below). Last updated 3 November 2026.</p>

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

      <h2>hitop sync test server, only if you sign in</h2>
      <p>hitop sync is optional and in testing. If you sign in to the test server at <code>sync.hitop.rws8.tech</code> to share collections, it keeps:</p>
      <ul>
        <li>Your email address, your devices' public keys and names, and hashes of your sign-in and session tokens (never the tokens).</li>
        <li>If you set a password: never the password itself. Your app turns it into two keys on your device; the server gets one of them and keeps only a slow hash of it, plus the settings your app needs to turn the password into keys again. The other key seals your device's keys (a key bundle the server stores but can't open), so the password unlocks your workspaces on a new device. If you sign in with Google or GitHub, the server keeps which Google or GitHub account it was (its id and email address); it never sees your Google or GitHub password and keeps no token from them.</li>
        <li>Your shared workspaces only as ciphertext: files are encrypted on your device before they are sent, and the server can't read them. It sees who is in which workspace, how many files there are, their sizes and when they change.</li>
        <li>Run history, only if your runs upload it (<code>hitop run --sync-report</code>): per run a status row the server can read (passed or failed, time, duration, request counts, the collection's and the environment's names, and failed request names only if the workspace's owners allow it), and the full report encrypted on your runner so the server can't read it. Alert settings (members' addresses, webhook URLs) are kept so the server can send alerts; an alert holds the status and a link, never a response or a secret value.</li>
        <li>Secret values never sync unless you share them: an environment's values shared with <code>hitop sync secrets share</code> are encrypted for the devices you pick, and the server can't read them.</li>
        <li>An audit log of security events: sign-ins and failed sign-ins, sign-outs, device keys, and in a workspace members added or removed, role changes, invites, key rotations and runner credentials. Each entry holds the time, the account, what it was done to, the first three parts of your IP address (the last part is dropped, IPv6 to its first 48 bits) and your app's user agent. Workspace owners can read their workspace's entries; you can read your own sign-ins. Entries are deleted after a year.</li>
      </ul>
      <p>Delete a workspace in the app or with <code>hitop sync</code>; email <a href="mailto:bohdaq@gmail.com">bohdaq@gmail.com</a> to have your account removed. A sync server someone else runs, for example your employer's, keeps what its operator configures.</p>

      <h2>HITOP Cloud plans, only if you buy one</h2>
      <p>Paid HITOP Cloud plans are sold through Stripe Managed Payments: Stripe (with Link, "Sold through Link") is the merchant of record. Stripe collects your name, email, billing address, payment details and tax information under its own privacy policy; HITOP never sees your card. The sync server keeps your organization's plan, seats, status and the Stripe customer, subscription, price and checkout ids, plus the ids and outcomes of Stripe's notifications (not their content). A Team trial records the organization, the account that started it and its email domain, so each gets one trial.</p>

      <h2>Children</h2>
      <p>HITOP does not knowingly collect information from anyone, including children under 13.</p>

      <h2>Changes</h2>
      <p>If this policy changes, the new version is posted at this address with a new date.</p>

      <h2>Contact</h2>
      <p>Email <a href="mailto:bohdaq@gmail.com">bohdaq@gmail.com</a> or open an issue on <a href=${GITHUB}>GitHub</a>.</p>
    </main>
    ${footer()}`;
}
