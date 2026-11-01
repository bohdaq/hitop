// Writes dist/<route> for every page with Lit SSR, plus the brand files,
// sitemap and robots.txt. Runs after `vite build` (needs the Vite manifest).
// The web app itself is built separately (frontend/, npm run build:web) and
// copied to dist/app/ by ../deploy.sh.
import { render } from "@lit-labs/ssr";
import { collectResult } from "@lit-labs/ssr/lib/render-result.js";
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
const pages = [homePage, pricingPage, downloadPage, privacyPage, notFoundPage, ...termsPages];
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
for (const f of ["favicon.svg", "apple-touch-icon.png", "favicon.png", "favicon-16x16.png", "favicon-32x32.png", "social-preview.png"]) {
  copyFileSync(join(publicAssets, f), join(dist, f));
}

const indexed = pages.filter((p) => !p.meta.noindex).map((p) => p.meta.path);
write(
  "sitemap.xml",
  `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${[...indexed, "/app/"]
    .map((p) => `  <url><loc>${ORIGIN}${p}</loc></url>`)
    .join("\n")}\n</urlset>\n`,
);
write("robots.txt", `User-agent: *\nAllow: /\nDisallow: /api/\nSitemap: ${ORIGIN}/sitemap.xml\n`);
console.log("brand files, sitemap and robots.txt written");
