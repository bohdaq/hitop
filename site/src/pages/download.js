// /download/: the free desktop builds and the CLI from GitHub Releases. The prerendered links
// point at the releases page; client/downloads.js swaps in direct links to the latest assets.
import { html } from "lit";
import { unsafeHTML } from "lit/directives/unsafe-html.js";
import { icon } from "../icons.js";
import { CHROME, FIREFOX, footer, GITHUB, header, IOS_APP, MAC_APP } from "../shell.js";

export const meta = {
  path: "/download/",
  title: "Download HITOP for macOS, Windows, Linux, Chrome, Firefox and the command line",
  description:
    "Free HITOP desktop builds for macOS, Windows and Linux, the Chrome and Firefox extensions, and the hitop CLI as a binary, a crate or a Docker image.",
};

const RELEASES = `${GITHUB}/releases`;

const DESKTOP = [
  ["desktop", "macOS", "Apple silicon and Intel, macOS 12 or later", "dmg", "universal\\.dmg$"],
  ["desktop", "Windows", "Windows 10 or later, installer. Microsoft Store: coming soon", "exe", "-setup\\.exe$"],
  ["desktop", "Linux AppImage", "Runs on most distributions", "AppImage", "\\.AppImage$"],
  ["desktop", "Debian and Ubuntu", "Install with apt", "deb", "\\.deb$"],
  ["desktop", "Fedora and openSUSE", "Install with dnf or zypper", "rpm", "\\.rpm$"],
];

const CLI = [
  ["Linux x86_64", "x86_64-unknown-linux-musl", "tar.gz"],
  ["Linux ARM64", "aarch64-unknown-linux-musl", "tar.gz"],
  ["macOS Apple silicon", "aarch64-apple-darwin", "tar.gz"],
  ["macOS Intel", "x86_64-apple-darwin", "tar.gz"],
  ["Windows x86_64", "x86_64-pc-windows-msvc", "zip"],
];

export function render() {
  return html`
    ${header(meta.path)}
    <main id="main" class="container page-text">
      <h1>Download HITOP</h1>
      <p class="lead">
        The quickest way in needs no download: <a href="/app/">open the web app</a>. For CI and the
        terminal, install the <a href="#cli"><code>hitop</code> command line tool</a>. The desktop app
        for macOS, Windows and Linux and the <a href="#extensions">Chrome and Firefox extensions</a>
        are free below; your collections stay on your device.
      </p>
      <p class="install">
        <a class="button-filled" href="/app/">${unsafeHTML(icon("browser"))} Open the web app</a>
        <a class="text-link" href="#cli">Install the CLI ${unsafeHTML(icon("arrow-right"))}</a>
      </p>

      <h2 id="stores">App Store</h2>
      <p>Free on the App Store for Mac, iPhone and iPad, updated by the store.</p>
      <p class="store-badges">
        <a class="store-badge" href=${MAC_APP}><img src="/badges/app-store.svg" alt="Download HITOP on the Mac App Store" width="120" height="40" /></a>
        <a class="store-badge" href=${IOS_APP}><img src="/badges/app-store.svg" alt="Download HITOP for iPhone and iPad on the App Store" width="120" height="40" /></a>
      </p>

      <h2 id="desktop">Desktop app</h2>
      <p data-release-note="desktop">From <a href=${`${RELEASES}?q=desktop-v&expanded=true`}>GitHub Releases</a>. On a Mac, you can also get the free <a href=${MAC_APP}>Mac App Store version</a>.</p>
      <ul class="downloads">
        ${DESKTOP.map(
          ([kind, name, note, label, pattern]) => html`<li>
            <a class="text-link" href=${`${RELEASES}?q=desktop-v&expanded=true`} data-release=${kind} data-asset=${pattern}>
              ${name} (.${label}) ${unsafeHTML(icon("arrow-right"))}
            </a>
            <span>${note}</span>
          </li>`,
        )}
      </ul>
      <p>
        The macOS, Windows and AppImage builds update themselves: HITOP checks for a new version at
        startup and once a day and asks before installing it (Settings, Updates). Versions up to
        1.2.0 can't: download the newest version once by hand, and from then on it updates itself.
        The .deb and .rpm packages update when you install the new package.
      </p>
      <details class="unsigned-note">
        <summary>Windows warns the first time you open it</summary>
        <p>
          The macOS build is signed and notarised by Apple. The Windows build isn't signed with a
          paid certificate yet: if SmartScreen says "Windows protected your PC", choose More info,
          then Run anyway.
        </p>
      </details>

      <h2 id="extensions">Browser extensions</h2>
      <p>
        The HITOP web app in a browser tab, one click from the toolbar. Free, no account needed, and the
        stores update it for you.
      </p>
      <ul class="downloads">
        <li>
          <a class="text-link" href=${CHROME}>Chrome Web Store ${unsafeHTML(icon("arrow-right"))}</a>
          <span>Chrome, Edge, Brave and other Chromium browsers</span>
        </li>
        <li>
          <a class="text-link" href=${FIREFOX}>Firefox Add-ons ${unsafeHTML(icon("arrow-right"))}</a>
          <span>Firefox 115 or later</span>
        </li>
      </ul>

      <h2 id="cli">Command line</h2>
      <p>Install with one line on Linux and macOS (the release binary, its SHA-256 checked, into <code>~/.local/bin</code>; <code>HITOP_VERSION</code> and <code>HITOP_INSTALL_DIR</code> change it), with Cargo, as a prebuilt binary, or run it from the Docker image:</p>
      <ul>
        <li><code>curl -fsSL https://hitop.rws8.tech/install.sh | sh</code> (<a href="/install.sh">read the script</a>)</li>
        <li><code>cargo install hitop</code></li>
        <li><code>cargo binstall hitop</code></li>
        <li><code>docker run --rm -v "$PWD:/work" ghcr.io/bohdaq/hitop run ./my-api</code></li>
      </ul>
      <p data-release-note="cli">Binaries from <a href=${`${RELEASES}?q=cli-v&expanded=true`}>GitHub Releases</a>; the Linux ones are static.</p>
      <ul class="downloads">
        ${CLI.map(
          ([name, target, ext]) => html`<li>
            <a class="text-link" href=${`${RELEASES}?q=cli-v&expanded=true`} data-release="cli" data-asset=${`^hitop-${target}\\.${ext.replace(".", "\\.")}$`}>
              ${name} ${unsafeHTML(icon("arrow-right"))}
            </a>
            <span><code>hitop-${target}.${ext}</code></span>
          </li>`,
        )}
      </ul>
    </main>
    ${footer()}`;
}
