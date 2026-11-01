// The HITOP mark, flat variant (no gradient, skill B4): the app icon's H and play arrow on a
// purple disc, for the header and 404. The favicon and app icons use the gradient master,
// brand/logo.svg (see brand/BRAND.md).
const PURPLE = "#6b55d6";

export function mark({ title = "", disc = PURPLE, glyph = "#ffffff" } = {}) {
  const label = title ? `role="img" aria-label="${title}"` : 'aria-hidden="true"';
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64" ${label}><circle cx="32" cy="32" r="30" fill="${disc}"/><path d="M18 20v24M18 32h10M28 20v24" fill="none" stroke="${glyph}" stroke-width="4" stroke-linecap="round" stroke-linejoin="round"/><path d="M36 24v16l12-8z" fill="${glyph}"/></svg>`;
}
