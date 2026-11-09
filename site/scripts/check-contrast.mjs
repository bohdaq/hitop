// HITOP Next A9: every --hitop-* meaning colour and the text roles must reach 4.5:1 on every
// surface they can sit on, in both themes (site/src/theme.css; WCAG 2.2 contrast).
//   node scripts/check-contrast.mjs   (exit 1 on a pair below 4.5:1)
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const css = fs.readFileSync(path.join(path.dirname(fileURLToPath(import.meta.url)), "../src/theme.css"), "utf8");
const role = (name) => {
  const m = new RegExp(`--${name}: light-dark\\((#[0-9a-fA-F]{6}), (#[0-9a-fA-F]{6})\\)`).exec(css);
  if (!m) throw new Error(`no light-dark value for --${name}`);
  return { light: m[1].toLowerCase(), dark: m[2].toLowerCase() };
};
const lum = (hex) => {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};
export const contrast = (a, b) => {
  const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
};

const SURFACES = ["md-sys-color-surface", "md-sys-color-surface-container-lowest", "md-sys-color-surface-container-low", "md-sys-color-surface-container", "md-sys-color-surface-container-high", "md-sys-color-surface-container-highest"].map(role);
const TEXT = [
  "md-sys-color-on-surface",
  "md-sys-color-on-surface-variant",
  "md-sys-color-primary",
  "md-sys-color-error",
  ...[...css.matchAll(/--(hitop-(?:method|pass|fail|warn|info)[a-z-]*): light-dark/g)].map((m) => m[1]),
];

let failed = 0;
for (const name of TEXT) {
  const fg = role(name);
  for (const theme of ["light", "dark"]) {
    const worst = Math.min(...SURFACES.map((s) => contrast(fg[theme], s[theme])));
    const ok = worst >= 4.5;
    if (!ok) failed++;
    console.log(`${ok ? "ok  " : "FAIL"} --${name} ${theme} ${fg[theme]} worst ${worst.toFixed(2)}:1`);
  }
}
process.exit(failed ? 1 : 0);
