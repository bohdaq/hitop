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

// The sizes inside each favicon.ico, as before.
const ICO_SIZES = [16, 32, 48, 64, 128, 256];

// [path from the repository root, size in px (a list for .ico), kind]. Sizes are the sizes these files always had
// (extension/icons/icon-96.png has been 192 x 192 since the first extension build; browsers scale it).
export const TARGETS = [
  ["site/public-assets/favicon.svg", 0, "svg"],
  ["site/public-assets/favicon-16x16.png", 16, "png"],
  ["site/public-assets/favicon-32x32.png", 32, "png"],
  ["site/public-assets/favicon.png", 192, "png"],
  ["site/public-assets/apple-touch-icon.png", 180, "png"],
  ["frontend/public/favicon.ico", ICO_SIZES, "ico"],
  ["frontend/public/logo192.png", 192, "png"],
  ["frontend/public/logo512.png", 512, "png"],
  ["desktop/src/logo192.png", 192, "png"],
  ["extension/icons/icon-48.png", 48, "png"],
  ["extension/icons/icon-96.png", 192, "png"],
  ["chrome-store-package/favicon.ico", ICO_SIZES, "ico"],
  ["chrome-store-package/icons/icon-16.png", 16, "png"],
  ["chrome-store-package/icons/icon-48.png", 48, "png"],
  ["chrome-store-package/icons/icon-128.png", 128, "png"],
  ["chrome-store-package/logo192.png", 192, "png"],
  ["chrome-store-package/logo512.png", 512, "png"],
  ...[16, 32, 48, 128, 192, 256, 300, 512, 1024].map((s) => [`brand/png/logo-${s}.png`, s, "png"]),
  ["brand/png/logo-mono-512.png", 512, "png-mono"],
];

const png = (svg, size) => Buffer.from(new Resvg(svg, { fitTo: { mode: "width", value: size } }).render().asPng());

// ICO holding one PNG image per size (Vista+ format; every browser reads it).
function ico(sizes) {
  const images = sizes.map((size) => png(master, size));
  const header = Buffer.alloc(6 + 16 * sizes.length);
  header.writeUInt16LE(0, 0); // reserved
  header.writeUInt16LE(1, 2); // type: icon
  header.writeUInt16LE(sizes.length, 4);
  let offset = header.length;
  sizes.forEach((size, i) => {
    const at = 6 + 16 * i;
    header.writeUInt8(size >= 256 ? 0 : size, at); // 0 means 256
    header.writeUInt8(size >= 256 ? 0 : size, at + 1);
    header.writeUInt8(0, at + 2); // no palette
    header.writeUInt8(0, at + 3);
    header.writeUInt16LE(1, at + 4); // planes
    header.writeUInt16LE(32, at + 6); // bits per pixel
    header.writeUInt32LE(images[i].length, at + 8);
    header.writeUInt32LE(offset, at + 12);
    offset += images[i].length;
  });
  return Buffer.concat([header, ...images]);
}

function render([, size, kind]) {
  if (kind === "svg") return Buffer.from(master);
  if (kind === "ico") return ico(size);
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
