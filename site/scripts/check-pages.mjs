// Checks on the built site (npm run build first; CI job "web"), for HITOP Cloud C3
// (spec/CLOUD_SERVICE.md): the security page says plainly what isn't done, no page still promises
// "no subscription", the pricing page shows the Cloud tiers, and the legal drafts stay drafts
// (banner, noindex, out of the sitemap, linked from no other page) until legal-status.js says
// they are approved.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { DRAFT, LEGAL_LINKS } from "../src/legal-status.js";

const site = join(dirname(fileURLToPath(import.meta.url)), "..");
const dist = join(site, "dist");
const failures = [];
const check = (ok, what) => {
  if (!ok) failures.push(what);
};
const text = (html) => html.replace(/<script[\s\S]*?<\/script>/g, " ").replace(/<style[\s\S]*?<\/style>/g, " ").replace(/<[^>]+>/g, " ").replace(/&#39;|&apos;/g, "'").replace(/&amp;/g, "&").replace(/\s+/g, " ");
const files = (dir) => readdirSync(dir).flatMap((f) => (statSync(join(dir, f)).isDirectory() ? files(join(dir, f)) : [join(dir, f)]));
const read = (path) => readFileSync(join(dist, path), "utf8");

// The two sentences that stay until they are untrue (spec section 7, Q11, Q13).
const security = text(read("security.html"));
for (const s of ["No external security review of HITOP's encryption has been done yet.", "HITOP Cloud runs on a single server shared with other sites."]) {
  check(security.includes(s), `security.html must say: ${s}`);
}

// No "no subscription" promise left, in the sources or the build.
for (const f of [...files(join(site, "src")), ...files(join(site, "content")), ...files(dist).filter((f) => f.endsWith(".html"))]) {
  const t = readFileSync(f, "utf8");
  check(!t.includes("Is there a subscription? No"), `${relative(site, f)} still says "Is there a subscription? No"`);
  check(!/no workspace sync/i.test(t), `${relative(site, f)} still says "no workspace sync"`);
}
const pricing = text(read("pricing.html"));
check(!/Is there a subscription\?\s*No\b/.test(pricing), "pricing.html: the subscription answer must not be No");
for (const p of ["$6", "$10", "$20", "HITOP Cloud", "Free forever", "Stripe", "plus applicable tax"]) check(pricing.includes(p), `pricing.html must show ${p}`);
check(/id="refunds"/.test(read("pricing.html")), "pricing.html needs the #refunds anchor (the portal links it)");

// Legal drafts.
const sitemap = read("sitemap.xml");
for (const [href] of LEGAL_LINKS) {
  const page = read(href.slice(1));
  if (DRAFT) {
    check(page.includes('<meta name="robots" content="noindex">'), `${href}: a draft must be noindex`);
    check(text(page).includes("Draft for review."), `${href}: a draft needs its banner`);
    check(!sitemap.includes(href), `${href}: a draft must not be in the sitemap`);
  } else {
    check(!text(page).includes("Draft for review."), `${href}: approved, but still shows the draft banner`);
  }
}
if (DRAFT) {
  const legal = new Set(LEGAL_LINKS.map(([h]) => h));
  for (const f of files(dist).filter((f) => f.endsWith(".html"))) {
    const path = `/${relative(dist, f)}`;
    if (legal.has(path)) continue;
    for (const href of legal) check(!read(path.slice(1)).includes(`href="${href}"`), `${path} links the draft ${href}`);
  }
}

if (failures.length) {
  console.error(failures.map((f) => `FAIL ${f}`).join("\n"));
  process.exit(1);
}
console.log(`site checks passed (legal pages ${DRAFT ? "are drafts" : "approved"})`);
