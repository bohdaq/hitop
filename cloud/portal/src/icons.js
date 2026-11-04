import { svg } from "lit";

// Small line icons (24px grid), the HITOP set's weight; swap in @lit-material/icon's registry later.
const icon = (d) => svg`<svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${d}</svg>`;

export const homeIcon = icon(svg`<path d="M3 11 12 4l9 7"/><path d="M5 10v10h14V10"/><path d="M10 20v-6h4v6"/>`);
export const workspacesIcon = icon(svg`<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 9h18M9 9v11"/>`);
export const orgIcon = icon(svg`<path d="M4 21V5a1 1 0 0 1 1-1h8a1 1 0 0 1 1 1v16"/><path d="M14 10h5a1 1 0 0 1 1 1v10"/><path d="M8 8h2M8 12h2M8 16h2M17 14h1M17 18h1M2 21h20"/>`);
export const keyIcon = icon(svg`<circle cx="8" cy="14" r="4"/><path d="M11 11 20 2M16 6l2 2M14 8l2 2"/>`);
export const personIcon = icon(svg`<circle cx="12" cy="8" r="4"/><path d="M4 21c0-4 3.6-6 8-6s8 2 8 6"/>`);
export const expandIcon = icon(svg`<path d="m6 9 6 6 6-6"/>`);
export const removeIcon = icon(svg`<path d="M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3"/>`);
export const downloadIcon = icon(svg`<path d="M12 4v12M6 11l6 6 6-6M4 20h16"/>`);
export const checkIcon = icon(svg`<path d="m5 12 5 5L20 7"/>`);
export const runsIcon = icon(svg`<path d="M4 19h16M4 15l4-4 4 3 8-8"/>`);
export const menuIcon = icon(svg`<path d="M4 7h16M4 12h16M4 17h16"/>`);
