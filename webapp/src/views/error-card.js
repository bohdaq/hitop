// UX2 (7): a failed request, connection or call as a headline and what to try next; the engine's
// own text stays under Details (errors.js decides the words).
import { html, nothing } from "lit";
import { explainError } from "../errors.js";

export function errorCard(raw, kind = "http") {
  const e = explainError(raw, { kind });
  return html`<div class="error-card" role="alert" data-known=${String(e.known)}>
    <lit-material-status status="error">${e.headline}</lit-material-status>
    ${e.next ? html`<p class="error-next">${e.next}</p>` : nothing}
    ${e.known
      ? html`<details class="error-details"><summary>Details</summary><p class="error-text mono">${e.raw}</p></details>`
      : html`<p class="error-text">${e.raw}</p>`}
  </div>`;
}
