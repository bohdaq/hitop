// /download/: the free desktop builds and the CLI from GitHub Releases. The prerendered links
// point at the releases page; client/downloads.js swaps in direct links to the latest assets.
import { html } from "lit";
import { unsafeHTML } from "lit/directives/unsafe-html.js";
import { icon } from "../icons.js";
import { footer, GITHUB, header, MAC_APP } from "../shell.js";

export const meta = {
  path: "/download/",
  title: "Download HITOP for macOS, Windows, Linux and the command line",
  description:
    "Free HITOP desktop builds for macOS, Windows and Linux, and the hitop CLI as a binary, a crate or a Docker image.",
};

const RELEASES = `${GITHUB}/releases`;

const DESKTOP = [
  ["desktop", "macOS", "Apple silicon and Intel, macOS 12 or later", "dmg", "universal\\.dmg$"],
  ["desktop", "Windows", "Windows 10 or later, installer", "exe", "-setup\\.exe$"],
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
        Free builds for macOS, Windows and Linux, and the <code>hitop</code> command line tool. Your
        collections stay on your device. Or skip the download and
        <a href="/app/">open the web app</a>.
      </p>

      <h2 id="desktop">Desktop app</h2>
      <p data-release-note="desktop">From <a href=${`${RELEASES}?q=desktop-v&expanded=true`}>GitHub Releases</a>.</p>
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
        These builds aren't signed with a paid certificate, so your system warns the first time you
        open them. On macOS, if it says the app is damaged, run
        <code>xattr -dr com.apple.quarantine /Applications/HITOP.app</code> once. On Windows, choose
        More info, then Run anyway. The signed <a href=${MAC_APP}>Mac App Store version</a> skips
        these steps and updates itself.
      </p>

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
