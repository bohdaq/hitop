// Writes dist/<route> for every page with Lit SSR, plus the brand files,
// sitemap and robots.txt. Runs after `vite build` (needs the Vite manifest).
// The web app itself is built separately (frontend/, npm run build:web) and
// copied to dist/app/ by ../deploy.sh.
import { render } from "@lit-labs/ssr";
import { collectResult } from "@lit-labs/ssr/lib/render-result.js";
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import "../src/components.js";
import { documentHtml, ORIGIN } from "../src/shell.js";
import * as homePage from "../src/pages/home.js";
import * as pricingPage from "../src/pages/pricing.js";
import * as privacyPage from "../src/pages/privacy.js";
import * as notFoundPage from "../src/pages/not-found.js";
import * as downloadPage from "../src/pages/download.js";
import { pages as termsPages } from "../src/pages/terms.js";
import { pages as legalPages } from "../src/pages/legal.js";

const site = join(dirname(fileURLToPath(import.meta.url)), "..");
const publicAssets = join(site, "public-assets");
const dist = join(site, "dist");
const manifest = JSON.parse(readFileSync(join(dist, ".vite", "manifest.json"), "utf8"));
const entry = manifest["src/client/main.js"];
const assets = { js: `/${entry.file}`, css: (entry.css ?? []).map((f) => `/${f}`) };

function write(path, content) {
  const file = join(dist, path);
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, content);
}

homePage.meta.jsonLd = homePage.structuredData();
const pages = [homePage, pricingPage, downloadPage, privacyPage, notFoundPage, ...termsPages, ...legalPages];
for (const page of pages) {
  const body = await collectResult(render(await page.render()));
  const out = page.meta.path.endsWith("/") ? `${page.meta.path}index.html` : page.meta.path;
  write(out, documentHtml({ ...page.meta, body, assets }));
}
console.log(pages.length, "pages");

// /app.html was the old entry to the web app; links to it keep working.
write(
  "app.html",
  `<!doctype html><html lang="en"><head><meta charset="utf-8"><title>HITOP web app</title><link rel="canonical" href="${ORIGIN}/app/"><meta http-equiv="refresh" content="0; url=/app/"></head><body><a href="/app/">Open the HITOP web app</a></body></html>\n`,
);

// The icons are renders of brand/logo.svg (brand/export.sh); favicon.svg is the master itself.
// install.sh: `curl -fsSL https://hitop.rws8.tech/install.sh | sh` installs the CLI from GitHub Releases.
for (const f of ["favicon.svg", "apple-touch-icon.png", "favicon.png", "favicon-16x16.png", "favicon-32x32.png", "social-preview.png", "install.sh"]) {
  copyFileSync(join(publicAssets, f), join(dist, f));
}

copyFileSync(join(publicAssets, "favicon.ico"), join(dist, "favicon.ico"));
// IndexNow key file (Bing, Yandex…): proves ownership when changed pages are announced.
write("2c32426256ab5e82b6a7fa63c90848c0.txt", "2c32426256ab5e82b6a7fa63c90848c0\n");

// When a guide last changed: its Markdown file's last commit.
const lastmod = (file) => {
  try {
    return execFileSync("git", ["-C", site, "log", "-1", "--format=%cs", "--", file], { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim() || null;
  } catch {
    return null;
  }
};
const indexedPages = pages.filter((p) => !p.meta.noindex);
write(
  "sitemap.xml",
  `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${[...indexedPages.map((p) => [p.meta.path, p.meta.source ? lastmod(p.meta.source) : null]), ["/app/", null]]
    .map(([p, date]) => `  <url><loc>${ORIGIN}${p}</loc>${date ? `<lastmod>${date}</lastmod>` : ""}</url>`)
    .join("\n")}\n</urlset>\n`,
);
// AI crawlers and assistants are welcome: the guides are written to be quoted.
const AI = ["GPTBot", "OAI-SearchBot", "ChatGPT-User", "ClaudeBot", "Claude-User", "Claude-SearchBot", "PerplexityBot", "Perplexity-User", "Google-Extended", "Applebot-Extended", "CCBot"];
write(
  "robots.txt",
  ["User-agent: *", "Allow: /", "Disallow: /api/", "", ...AI.flatMap((ua) => [`User-agent: ${ua}`, "Allow: /", "Disallow: /api/", ""]), `Sitemap: ${ORIGIN}/sitemap.xml`, ""].join("\n"),
);

// Markdown copies of the guides, and llms.txt listing them (llmstxt.org).
let copies = 0;
for (const p of pages) {
  if (!p.meta.markdown) continue;
  write(p.meta.markdown.path, p.meta.markdown.text);
  copies++;
}
const guides = pages.filter((p) => p.meta.markdown);
const others = indexedPages.filter((p) => !p.meta.markdown && p.meta.path !== "/terms/");
write(
  "llms.txt",
  [
    "# HITOP",
    "",
    `> ${homePage.meta.description}`,
    "",
    "HITOP is an open-source (MIT) HTTP API client: requests, collections, environments and JavaScript scripts, in the browser (no account, no CORS errors), as browser extensions, a CLI for CI, and native macOS and iOS apps. Collections are plain files that live in git.",
    "",
    `Website: ${ORIGIN}/ · Web app: ${ORIGIN}/app/ · Source: https://github.com/bohdaq/hitop · License: MIT`,
    "",
    "## Guides",
    "",
    ...guides.map((p) => `- [${p.meta.title.replace(/ · HITOP$/, "")}](${ORIGIN}${p.meta.markdown.path}): ${p.meta.description}`),
    "",
    "## Pages",
    "",
    ...others.map((p) => `- [${p.meta.title.replace(/ · HITOP$/, "")}](${ORIGIN}${p.meta.path}): ${p.meta.description}`),
    "",
    "## Optional",
    "",
    "- [Scripting guide](https://github.com/bohdaq/hitop/blob/main/SCRIPTING_GUIDE.md): pre-request and test scripts",
    "- [Variables guide](https://github.com/bohdaq/hitop/blob/main/VARIABLES_GUIDE.md): environments and variables",
    "- [Changelog](https://github.com/bohdaq/hitop/blob/main/CHANGELOG.md): changes per release",
    "",
  ].join("\n"),
);
console.log(`brand files, sitemap, robots.txt, llms.txt and ${copies} Markdown copies written`);
