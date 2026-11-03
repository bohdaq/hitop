// The legal and security pages (spec/CLOUD_SERVICE.md section 7, tranche C3) are drafts until the
// owner (and a lawyer) approve them. While DRAFT is true each of them carries a "draft" banner and
// `noindex`, stays out of the sitemap, and no other page links to them. Approving them = setting
// this to false (spec section 11, the C3 handoff); nothing else changes.
export const DRAFT = true;

// The pages, for the footer once they are approved.
export const LEGAL_LINKS = [
  ["/legal/terms.html", "Terms"],
  ["/legal/privacy.html", "HITOP Cloud privacy"],
  ["/legal/dpa.html", "DPA"],
  ["/legal/sla.html", "SLA"],
  ["/legal/subprocessors.html", "Sub-processors"],
  ["/security.html", "Security"],
];
