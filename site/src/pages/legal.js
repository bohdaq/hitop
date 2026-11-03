// The HITOP Cloud legal pages and the security page (spec/CLOUD_SERVICE.md section 7, tranche C3):
// /legal/{terms,privacy,dpa,sla,subprocessors}.html and /security.html, written in Markdown in
// site/content/legal/<name>.md (front matter: path, title, description). Drafts until the owner
// approves them (legal-status.js DRAFT): a banner, noindex, and no links from other pages.
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { html, nothing } from "lit";
import { unsafeHTML } from "lit/directives/unsafe-html.js";
import { marked } from "marked";
import { DRAFT } from "../legal-status.js";
import { footer, header } from "../shell.js";

const SOURCE = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "content", "legal");

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
  return { meta, body };
}

export const pages = readdirSync(SOURCE)
  .filter((f) => f.endsWith(".md"))
  .sort()
  .map((file) => {
    const { meta, body } = read(file);
    return {
      meta: { path: meta.path, title: meta.title, description: meta.description, noindex: DRAFT },
      render: () => html`
        ${header(meta.path)}
        <main id="main" class="container page-text legal">
          ${DRAFT
            ? html`<p class="draft-banner" role="note"><strong>Draft for review.</strong> This page is not in force yet: it is waiting for the owner's and a lawyer's review before HITOP Cloud's paid plans open.</p>`
            : nothing}
          <article class="prose">${unsafeHTML(body)}</article>
          <p class="legal-date">Draft of 3 November 2026.</p>
        </main>
        ${footer()}`,
    };
  });
