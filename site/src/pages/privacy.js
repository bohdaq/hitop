// /privacy.html: the App Store listings link here (through the old GitHub
// Pages address, which redirects), so the path must not change.
import { html } from "lit";
import { footer, GITHUB, header } from "../shell.js";

export const meta = {
  path: "/privacy.html",
  title: "Privacy policy · HITOP",
  description:
    "HITOP has no tracking and needs no account. Collections and history stay on your device. What an optional HITOP Cloud account keeps, how to delete it, and how the web app's request proxy and the usage ping you can turn on work.",
};

export function render() {
  return html`
    ${header(meta.path)}
    <main id="main" class="container page-text">
      <h1>Privacy policy</h1>
      <p class="lead">HITOP doesn't track you, show ads or sell personal data, and you never need an account. The one exception is optional: if you sign in to HITOP Cloud to sync or share collections, it keeps your account and what you sync (see below); you can delete the account in the app at any time. Last updated 21 November 2026.</p>

      <h2>What stays on your device</h2>
      <p>Everything you enter, including collections, requests, headers, variables, scripts and request history, is stored only on your device:</p>
      <ul>
        <li>macOS app and CLI on macOS: <code>~/Library/Application Support/hitop/</code></li>
        <li>CLI on Linux: <code>~/.config/hitop/</code></li>
        <li>CLI on Windows: <code>%APPDATA%\\hitop\\</code></li>
        <li>iOS app: the app's own storage on your iPhone or iPad</li>
        <li>Android app: the app's own storage on your phone or tablet (secret values sealed with a key in the Android Keystore); it is left out of Android's cloud backups and device transfers</li>
        <li>Web app and browser extensions: your browser's local storage</li>
      </ul>
      <p>None of it leaves your device unless you export it yourself, or sign in to HITOP Cloud and sync it (below).</p>

      <h2>Requests you send</h2>
      <p>The Mac app, the iOS and Android apps, the CLI and the browser extensions send each request straight from your device to the address you entered. HITOP's servers are not involved.</p>
      <p>The web app at hitop.rws8.tech sends requests to public addresses through the HITOP request proxy on the same server, because browsers block most cross origin API calls. For those requests:</p>
      <ul>
        <li>The proxy receives the method, URL, headers and body you send, makes the request, and returns the response to your browser.</li>
        <li>It does not store requests, responses, headers or bodies, and does not log them. The web server's access log records your IP address, the time and the path <code>/api/proxy</code>, not where the request went.</li>
        <li>It refuses private and internal addresses. Requests to localhost and other private addresses are never sent to the proxy; your browser sends them itself.</li>
        <li>Don't send secrets you are not comfortable passing through a server you don't run. For those, use the Mac, iOS or Android app, the CLI or an extension.</li>
      </ul>

      <h2>No tracking</h2>
      <p>HITOP contains no analytics SDKs, crash reporting, advertising identifiers, session recording or fingerprinting, and no third party SDKs apart from Google's in the Android app (below). The website sets no cookies. If you choose a light or dark theme, your browser keeps that choice in local storage under <code>hitop-theme</code>.</p>

      <h2>Usage ping, only if you turn it on</h2>
      <p>The CLI (from version 0.3.0) and the free desktop builds from GitHub can send one anonymous ping a week, only if you turn it on. It is off by default; the desktop app asks once, with No as the default answer, and the CLI only sends it after <code>hitop telemetry on</code>. The Mac App Store, iOS and Android apps, the web app and the browser extensions don't contain this code at all.</p>
      <ul>
        <li>The ping holds a random install id made on your device, which surface it is (CLI or desktop), the version and the operating system family. Nothing else: no requests, URLs, collections or account.</li>
        <li>It goes to <code>https://hitop.rws8.tech/api/ping</code>. The server keeps only daily counts. It uses the install id to count each install once a week, through a hash with a salt that is thrown away every week, so installs can't be followed from week to week. It doesn't store your IP address; the web server's access log records it with the time and the path, as for any page.</li>
        <li>Turn it off at any time with <code>hitop telemetry off</code> or in the desktop app's settings. <code>hitop telemetry status</code> shows what would be sent.</li>
      </ul>

      <h2>On-device AI in the Android app</h2>
      <p>On phones that have Gemini Nano, the Android app can explain a response, suggest tests and turn a description into a request. The model runs on the phone through Google's ML Kit and Android's on-device AI service: what it reads (the response, with secret values and credentials hidden) and what it answers stay on the phone. Turning a curl command into a request needs no model.</p>
      <p>ML Kit is Google's library, not HITOP's, and it sends Google diagnostics about its own use whenever the app checks for the model (at start) or uses it: the phone's model and Android version, HITOP's version, timings, sizes and error codes, and an identifier per installation. It doesn't send what the model reads or writes. Google describes this at <a href="https://developers.google.com/ml-kit/android-data-disclosure">ML Kit data disclosure</a>. The Android app's other Google library, for signing in with Google, is used only when you choose it.</p>

      <h2>HITOP Cloud account, only if you sign in</h2>
      <p>HITOP Cloud is optional: every app works without an account. If you sign in (with Apple, Google or GitHub, a password, an emailed code or your company's single sign-on) at <code>sync.hitop.rws8.tech</code>, HITOP Cloud keeps:</p>
      <ul>
        <li>Your account: your email address (with Sign in with Apple, the relay address if you hide yours) and an account id. HITOP doesn't keep the name Apple or Google sends. For each Apple, Google, GitHub or single sign-on account you sign in with, which one it is (its id and email address); never its password. With Sign in with Apple, the server also keeps the token Apple gives it, sealed, only so that deleting your account can revoke HITOP's access at Apple.</li>
        <li>Your devices: a name (the apps send "HITOP on iPhone", "HITOP on iPad", "HITOP on Pixel 9" (the Android phone's model) and the like, not your device's own name), each device's public key, when it was added and last used, and hashes of your sign-in and session tokens (never the tokens).</li>
        <li>If you set a password: never the password itself. Your app turns it into two keys on your device; the server gets one of them and keeps only a slow hash of it, plus the settings your app needs to turn the password into keys again. The other key seals your device's keys (a key bundle the server stores but can't open). A recovery phrase works the same way.</li>
        <li>Your synced personal library: the collections, environments and globals you keep in HITOP Cloud, and the last versions of each saved request, so they are the same on every device you sign in on. HITOP Cloud can read them. Secret values are sealed under a key only the server holds, and are not kept at all on a server without one.</li>
        <li>Shared collections only as ciphertext: they are encrypted end to end on your devices before they are sent, and the server can't read them. It sees who is in which shared collection, how many files there are, their sizes and when they change.</li>
        <li>Organizations: the ones you created or joined, their names, members and roles, the plan and seats an organization has, and daily usage counts (devices, storage, invites).</li>
        <li>If you connect GitHub to save a collection to a repository: the GitHub account (its login) and installation HITOP's GitHub App was given, and which repository, branch and folder each collection goes to. If you set up a monitor or a mock server for a collection: its settings (schedule, environment, the addresses alerts go to) and recent results.</li>
        <li>Run history, only if your runs upload it: per run a status row the server can read (passed or failed, time, duration, request counts, the collection's and the environment's names, and failed request names only if the owners allow it), and the full report encrypted so the server can't read it. Alert settings (members' addresses, webhook URLs) are kept so the server can send alerts.</li>
        <li>An audit log of security events: sign-ins and failed sign-ins, sign-outs, device keys, account deletion, and in a shared collection or organization members added or removed, role changes, invites and key rotations. Each entry holds the time, the account and its email address, what it was done to, the first three parts of your IP address (the last part is dropped, IPv6 to its first 48 bits) and your app's user agent. Entries are deleted after a year.</li>
      </ul>
      <p>What HITOP Cloud doesn't do: no tracking, no advertising, no analytics, and your data is never sold or shared for marketing. The requests you send and their responses never go through HITOP Cloud (the web app's proxy is above), and it doesn't see your request history or your device's Keychain.</p>

      <h3>Deleting your account</h3>
      <p>In the iOS or Android app, the web app or the desktop app: <b>Account → Delete account</b>. You can also email <a href="mailto:bohdaq@gmail.com">bohdaq@gmail.com</a> and we delete it for you. Deleting is immediate and can't be undone:</p>
      <ul>
        <li>Deleted at once: your account, its sign-ins and sessions, access tokens, password and recovery data, devices and their keys, your synced personal library and its versions, your personal organization, organizations and shared collections only you were in (with their data), your memberships of the others, and pending invites to your address. Shared collections you leave this way get a new key from their owners' apps. Your Sign in with Apple token is revoked at Apple. If you installed HITOP's GitHub App, it stays installed on your GitHub account until you uninstall it there (GitHub → Settings → Applications); HITOP no longer uses it.</li>
        <li>If you are the only owner of an organization or shared collection that other people use, the app asks you to make someone else an owner, or remove the others, first, so their work isn't deleted with your account.</li>
        <li>An organization you delete this way stops its paid plan at once: nothing more is charged. Stripe keeps its own records of past payments as the law requires.</li>
        <li>What stays for a while: audit log entries that name you are deleted when they are a year old. Copies of the database the server keeps (made when it is upgraded, and encrypted backups once they are turned on) are deleted when they are 35 days old, and your old data goes with them. Collections and requests saved on your own devices stay there until you delete them.</li>
      </ul>
      <p>Without an account nothing needs deleting: remove the app, or clear the web app's site data in your browser. A sync server someone else runs, for example your employer's, keeps what its operator configures.</p>

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
