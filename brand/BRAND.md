# HITOP brand

One name, one line, one mark, on every release: site, web app, desktop (GitHub and Mac App Store
builds), iOS apps, browser extensions, CLI help and READMEs, Docker image labels and hitop-action.
Owner decision, 2026-11-01.

## Name

- **HITOP**, all capitals, wherever a person sees the name of the product or an app: window
  titles, Tauri `productName`, iOS `CFBundleDisplayName`, extension `name` and `default_title`,
  PWA manifests, HTML `<title>`, Open Graph tags, store listings, prose about the product ("the
  HITOP web app"). Where a sentence is about the CLI program, `hitop` (the command) is fine.
  No suffixes in the name itself ("HITOP - HTTP Client", "HITOP API Tester" and friends are gone).
- **`hitop`**, lowercase in code style, is the command and every identifier: the binary, crate and
  npm package names, `ghcr.io/bohdaq/hitop`, `bohdaq/hitop-action`, storage directories, keychain
  service, file names. Identifiers never change: bundle id `tech.rws8.hitop` (desktop),
  `hitop.rws8.tech.HiTop` (iOS app), `hitop.rws8.tech.HitopCompanion` (+ `.Widget`), the extension
  ids, Xcode target and folder names (`ios/HiTop`, `HiTopApp`). Renaming them breaks installs and
  store listings.
- **HITOP Runs**: the iOS companion (`ios/HitopCompanion`) and its widget, while the companion is a
  separate App Store app (its bundle id today). Two home-screen icons both labelled "HITOP" would be
  indistinguishable. If the owner ships it as the next version of the existing iOS app instead
  (ROADMAP T26 handoff), its display name becomes "HITOP".
- The App Store listings are still called "HiTop API Client" (Mac) and "HiTop HTTP Client" (iOS)
  until the owner renames them in App Store Connect (ROADMAP section 6).

## Tagline

> The API client your terminal, your CI and your AI agent can all drive.

The positioning line of the README and site (T11), 70 characters. It is the short description in
every manifest and description field: extension and PWA manifests, Tauri `shortDescription`, crate
descriptions, the CLI's `--help`, the Docker image description. Store limits: Chrome name ≤ 45 and
short description ≤ 132 characters; App Store name and subtitle ≤ 30 each, so the App Store subtitle
is a shorter line (ROADMAP section 6).

## Mark

`brand/logo.svg` is the master: a disc with a white "H" and play arrow, linear gradient from
`#667eea` (top left) to `#764ba2` (bottom right), on a transparent 64 × 64 canvas.

| Variant | File | Use |
| --- | --- | --- |
| Gradient (master) | `brand/logo.svg` | app icons, favicons, extension icons, anything raster |
| Flat | `site/src/brand/mark.js` (`#6b55d6` disc) | the site header and 404 only: the rws8.tech design system draws no gradients |
| One color | `brand/logo-mono.svg` (`#1f1d2b`, glyph cut out) | print, single-color badges, places that recolor (change `fill`) |

Colors: `#667eea` and `#764ba2` (gradient), `#6b55d6` (flat), `#ffffff` (glyph), `#1f1d2b` (one color).

## Where each asset is used

`brand/export.sh` writes all of these from `brand/logo.svg` (resvg, pinned in `package-lock.json`,
renders the same bytes on every machine). Sizes are what each file always was.

| File | Size | Used by |
| --- | --- | --- |
| `site/public-assets/favicon.svg` | copy of the master | site and web app favicon (`/favicon.svg`, prerender copies it) |
| `site/public-assets/favicon-16x16.png`, `favicon-32x32.png`, `favicon.png` | 16, 32, 192 | site |
| `site/public-assets/apple-touch-icon.png` | 180 | site, web app |
| `frontend/public/favicon.ico` | 16, 32, 48, 64, 128, 256 | extension pages (React app) |
| `frontend/public/logo192.png`, `logo512.png` | 192, 512 | extension pages, Firefox/Edge manifest from `manifest_extension.json` |
| `desktop/src/logo192.png` | 192 | React desktop UI sidebar |
| `extension/icons/icon-48.png`, `icon-96.png` | 48, 192 | Firefox extension (`icon-96.png` has always been 192 × 192) |
| `chrome-store-package/icons/icon-16.png`, `icon-48.png`, `icon-128.png` | 16, 48, 128 | Chrome Web Store package |
| `chrome-store-package/favicon.ico`, `logo192.png`, `logo512.png` | as in `frontend/public` | Chrome Web Store package |
| `brand/png/logo-{16,32,48,128,192,256,300,512,1024}.png`, `logo-mono-512.png` | | exports for store listings (300 = Edge store logo), docs, press |

Not written by default:

- `desktop/src-tauri/icons/` (icns, ico, Windows Store and mobile sets): made by Tauri's generator
  from the same master; `brand/export.sh --tauri` runs `npx @tauri-apps/cli icon brand/png/logo-1024.png`.
  Checked 2026-11-01: they match a fresh render (< 0.1 % of pixels differ, anti-aliasing only).
- iOS `AppIcon.appiconset` (`ios/HiTop`, `ios/HitopCompanion`): 1024 × 1024 on an opaque
  background (App Store icons may have no alpha), plus dark and tinted variants. Same mark; redo
  them in Xcode or an icon tool from `brand/png/logo-1024.png` if the master changes.
- `site/public-assets/social-preview.png` (1280 × 640, GitHub social preview and `og:image`): the
  gradient mark and "HITOP" on a dark background, made by hand.

## Changing the mark

1. Edit `brand/logo.svg` (and `logo-mono.svg`, `site/src/brand/mark.js` to match).
2. `brand/export.sh` (add `--tauri` for the desktop set), check the result, commit every changed file.
3. Redo the iOS app icons and the social preview by hand; upload new icons to the stores (ROADMAP section 6).

CI job `brand` runs `brand/check.sh`: `brand/export.sh --check` (each file above must equal the
render byte for byte) and a search for the old app names, which fails the build if one comes back.
