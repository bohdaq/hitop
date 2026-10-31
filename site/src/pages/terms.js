// /terms/: the guide articles, at the same URLs as on the old site. The
// article bodies are read from docs/terms/*.html at build time (their
// `.content` block), stripped of emoji and inline styles, and rendered in the
// site shell. Prerender only.
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { html } from "lit";
import { unsafeHTML } from "lit/directives/unsafe-html.js";
import { icon } from "../icons.js";
import { footer, header } from "../shell.js";

const SOURCE = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..", "docs", "terms");

const EMOJI = /[\p{Extended_Pictographic}\u{FE0F}\u{200D}]\s?/gu;

function clean(markup) {
  return markup
    .replace(EMOJI, "")
    .replace(/\sstyle="[^"]*"/g, "")
    .replace(/class="highlight-box"/g, 'class="callout"')
    .replace(/class="comparison-table"/g, 'class="table"')
    .trim();
}

// The `.content` div's inner HTML: from its opening tag to the CTA block.
function articleBody(source) {
  const start = source.indexOf('<div class="content">');
  const end = source.indexOf('<div class="cta">');
  if (start < 0 || end < 0) throw new Error("guide page without .content/.cta");
  const inner = source.slice(start + '<div class="content">'.length, end);
  return clean(inner.replace(/<\/div>\s*$/, ""));
}

function read(file) {
  const source = readFileSync(join(SOURCE, file), "utf8");
  const title = source.match(/<title>([^<]*)<\/title>/)[1].split(" - HITOP")[0].trim();
  const description = source.match(/name="description" content="([^"]*)"/)[1];
  return { file, title, description, body: articleBody(source) };
}

const GUIDES = readdirSync(SOURCE)
  .filter((f) => f.endsWith(".html") && f !== "index.html")
  .sort()
  .map(read);

function guidePage(guide) {
  const meta = { path: `/terms/${guide.file}`, title: `${guide.title} · HITOP`, description: guide.description };
  return {
    meta,
    render: () => html`
      ${header(meta.path)}
      <main id="main" class="container page-text guide">
        <p class="breadcrumb"><a href="/terms/">Guides</a></p>
        <h1>${guide.title}</h1>
        <p class="lead">${guide.description}</p>
        <article class="prose">${unsafeHTML(guide.body)}</article>
        <div class="guide-cta">
          <a class="button-filled" href="/app/">${unsafeHTML(icon("browser"))} Open the web app</a>
          <a class="text-link" href="/terms/">All guides ${unsafeHTML(icon("arrow-right"))}</a>
        </div>
      </main>
      ${footer()}`,
  };
}

const indexMeta = {
  path: "/terms/",
  title: "API testing guides · HITOP",
  description: "Guides to API testing with HITOP: REST testing, HTTP clients, collections, automation, offline and browser testing, and moving from Postman.",
};

const indexPage = {
  meta: indexMeta,
  render: () => html`
    ${header(indexMeta.path)}
    <main id="main" class="container">
      <section class="page-hero">
        <h1 class="hero-heading">Guides to testing <br />HTTP APIs</h1>
        <p class="lead">${indexMeta.description}</p>
      </section>
      <div class="tiles">
        ${GUIDES.map(
          (g) => html`
            <a class="tile" href=${`/terms/${g.file}`}>
              <h2 class="tile-title">${g.title}</h2>
              <p>${g.description}</p>
            </a>`,
        )}
      </div>
    </main>
    ${footer()}`,
};

export const pages = [indexPage, ...GUIDES.map(guidePage)];
export const paths = pages.map((p) => p.meta.path);
