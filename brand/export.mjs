// Renders every HITOP icon in the repository from brand/logo.svg, so they can't drift.
//
//   node brand/export.mjs           write every file below
//   node brand/export.mjs --check   write nothing; exit 1 if a file differs from what it would write
//
// resvg (pinned in package-lock.json) renders the same bytes on every machine, so the CI check
// compares files byte for byte. See brand/BRAND.md.
import { Resvg } from "@resvg/resvg-js";
import { readFileSync, writeFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const brand = dirname(fileURLToPath(import.meta.url));
const root = join(brand, "..");
const master = readFileSync(join(brand, "logo.svg"), "utf8");

// [path from the repository root, size in px, kind]. Sizes are the sizes these files always had.
export const TARGETS = [
  ["site/public-assets/favicon.svg", 0, "svg"],
  ["site/public-assets/favicon-16x16.png", 16, "png"],
  ["site/public-assets/favicon-32x32.png", 32, "png"],
  ["site/public-assets/favicon.png", 192, "png"],
  ["site/public-assets/apple-touch-icon.png", 180, "png"],
  ...[16, 32, 48, 128, 192, 256, 300, 512, 1024].map((s) => [`brand/png/logo-${s}.png`, s, "png"]),
  ["brand/png/logo-mono-512.png", 512, "png-mono"],
  ...msixAssets(),
  ...androidAssets(),
];

// Android adaptive launcher icon (android/app/src/main/res/mipmap-*): a 108 dp square per density, the logo's
// gradient as the background layer, its white H and play arrow as the foreground (and the monochrome layer for
// themed icons), scaled so the logo's disc (60 of its 64 units) spans the 72 dp the launcher's mask shows.
function androidAssets() {
  const densities = [["mdpi", 108], ["hdpi", 162], ["xhdpi", 216], ["xxhdpi", 324], ["xxxhdpi", 432]];
  return densities.flatMap(([d, px]) => ["background", "foreground", "monochrome"].map((layer) =>
    [`android/app/src/main/res/mipmap-${d}/ic_launcher_${layer}.png`, [px, layer], "android"]));
}

const glyph = master.match(/<path[\s\S]*?\/>\s*<!--[\s\S]*?-->\s*<path[\s\S]*?\/>/)[0];
function android([px, layer]) {
  const scale = 72 / 60;
  const offset = (108 - 64 * scale) / 2;
  const body = layer === "background"
    ? `<defs><linearGradient id="g" x1="0%" y1="0%" x2="100%" y2="100%"><stop offset="0%" stop-color="#667eea"/><stop offset="100%" stop-color="#764ba2"/></linearGradient></defs><rect width="108" height="108" fill="url(#g)"/>`
    : `<g transform="translate(${offset} ${offset}) scale(${scale})">${glyph}</g>`;
  return png(`<svg width="108" height="108" viewBox="0 0 108 108" fill="none" xmlns="http://www.w3.org/2000/svg">${body}</svg>`, px);
}

// Microsoft Store (MSIX) package images, desktop/msstore/Assets (AppxManifest.xml names the plain file; makepri
// indexes the scale and targetsize variants). [file, width, height, mark size as a share of the shorter side].
// Square44x44Logo is the taskbar, Start and title bar icon: the targetsize sets (plain and altform-unplated) are
// what Windows shows unplated at those pixel sizes. Tiles keep the mark at 60 % on a transparent tile.
function msixAssets() {
  const list = [
    ["Square44x44Logo.png", 44, 44, 1],
    ["Square44x44Logo.scale-200.png", 88, 88, 1],
    ...[16, 24, 32, 48, 256].flatMap((s) => [
      [`Square44x44Logo.targetsize-${s}.png`, s, s, 1],
      [`Square44x44Logo.targetsize-${s}_altform-unplated.png`, s, s, 1],
    ]),
    ["Square150x150Logo.png", 150, 150, 0.6],
    ["Square150x150Logo.scale-200.png", 300, 300, 0.6],
    ["Wide310x150Logo.png", 310, 150, 0.6],
    ["Wide310x150Logo.scale-200.png", 620, 300, 0.6],
    ["StoreLogo.png", 50, 50, 1],
    ["StoreLogo.scale-200.png", 100, 100, 1],
  ];
  return list.map(([name, w, h, share]) => [`desktop/msstore/Assets/${name}`, [w, h, share], "tile"]);
}

// The master's drawing (everything inside its <svg>), placed on a w × h transparent canvas at `share` of the
// shorter side, centred: the MSIX tiles.
const inner = master.replace(/^[\s\S]*?<svg[^>]*>/, "").replace(/<\/svg>\s*$/, "");
function tile([w, h, share]) {
  const s = Math.min(w, h) * share;
  const svg = `<svg width="${w}" height="${h}" viewBox="0 0 ${w} ${h}" fill="none" xmlns="http://www.w3.org/2000/svg">` +
    `<g transform="translate(${(w - s) / 2} ${(h - s) / 2}) scale(${s / 64})">${inner}</g></svg>`;
  return png(svg, w);
}

const png = (svg, size) => Buffer.from(new Resvg(svg, { fitTo: { mode: "width", value: size } }).render().asPng());

function render([, size, kind]) {
  if (kind === "svg") return Buffer.from(master);
  if (kind === "tile") return tile(size);
  if (kind === "android") return android(size);
  if (kind === "png-mono") return png(readFileSync(join(brand, "logo-mono.svg"), "utf8"), size);
  return png(master, size);
}

const check = process.argv.includes("--check");
const stale = [];
for (const target of TARGETS) {
  const file = join(root, target[0]);
  const bytes = render(target);
  if (check) {
    if (!existsSync(file) || !readFileSync(file).equals(bytes)) stale.push(target[0]);
  } else {
    mkdirSync(dirname(file), { recursive: true });
    writeFileSync(file, bytes);
  }
}
if (check) {
  if (stale.length) {
    console.error(`These icons differ from brand/logo.svg; run \`npm --prefix brand ci && node brand/export.mjs\` and commit:\n  ${stale.join("\n  ")}`);
    process.exit(1);
  }
  console.log(`${TARGETS.length} icon files match brand/logo.svg`);
} else {
  console.log(`wrote ${TARGETS.length} files under ${relative(process.cwd(), root) || "."}`);
}
