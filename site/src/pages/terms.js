// /terms/: the guide articles, at the same URLs as on the old site. Written
// in Markdown in site/content/guides/<name>.md (front matter: title,
// description) and served at /terms/<name>.html. Prerender only.
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { html } from "lit";
import { unsafeHTML } from "lit/directives/unsafe-html.js";
import { marked } from "marked";
import { icon } from "../icons.js";
import { footer, header } from "../shell.js";

const SOURCE = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "content", "guides");

function read(file) {
  const source = readFileSync(join(SOURCE, file), "utf8");
  const m = source.match(/^---\n([\s\S]*?)\n---\n/);
  if (!m) throw new Error(`${file}: missing front matter`);
  const meta = Object.fromEntries(
    m[1].split("\n").map((line) => {
      const at = line.indexOf(":");
      return [line.slice(0, at).trim(), JSON.parse(line.slice(at + 1).trim())];
    }),
  );
  const body = marked.parse(source.slice(m[0].length)).replace(/<table>/g, '<table class="table">');
  return { file: file.replace(/\.md$/, ".html"), title: meta.title, description: meta.description, body };
}

const GUIDES = readdirSync(SOURCE)
  .filter((f) => f.endsWith(".md"))
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
