import { css } from "lit";

/** Shared by every page: the title row with the page's one primary action, sections, hints, cards. */
export const pageStyles = css`
  :host {
    display: block;
    font-size: var(--text-sm);
    line-height: var(--leading-sm);
  }
  .title-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-200);
    margin-bottom: var(--space-300);
  }
  .title-row > div {
    min-width: 0;
  }
  h1 {
    margin: 0;
    font-size: var(--text-3xl);
    line-height: var(--leading-3xl);
    font-weight: 600;
    letter-spacing: -0.01em;
    text-wrap: balance;
  }
  h2 {
    margin: 0 0 var(--space-100);
    font-size: var(--text-xl);
    line-height: var(--leading-xl);
    font-weight: 600;
  }
  h3 {
    margin: 0 0 var(--space-75);
    font-size: var(--text-base);
    line-height: var(--leading-base);
    font-weight: 600;
  }
  section + section {
    margin-top: var(--space-500);
  }
  .lead,
  .hint {
    margin: 0;
    color: var(--md-sys-color-on-surface-variant);
    text-wrap: pretty;
  }
  .lead {
    max-width: 64ch;
    margin-top: var(--space-75);
  }
  .hint {
    font-size: var(--text-sm);
    line-height: var(--leading-sm);
  }
  p {
    margin: 0 0 var(--space-100);
  }
  a {
    color: var(--md-sys-color-primary);
  }
  code {
    font-family: var(--font-mono);
    font-size: 0.9375em;
  }
  .mono {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
  }
  .card {
    display: grid;
    gap: var(--space-200);
    padding: var(--space-300);
    border: 1px solid var(--md-sys-color-outline-variant);
    border-radius: 16px;
    background: var(--md-sys-color-surface-container-lowest);
  }
  .card.danger {
    border-color: var(--md-sys-color-error);
  }
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: var(--space-200);
  }
  .grid {
    display: grid;
    gap: var(--space-200);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-200);
  }
  .row.between {
    justify-content: space-between;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--space-75);
  }
  form.grid {
    max-width: 36rem;
  }
  /* An outlined field's label floats above it: room under the text before it. */
  form.row,
  p + .row {
    margin-top: var(--space-200);
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
  }
  .refusal {
    margin-bottom: var(--space-200);
  }
  .stack > * + * {
    margin-top: var(--space-100);
  }
  lit-material-skeleton {
    margin: var(--space-100) 0;
  }
  /* Until @lit-material/switch 1.2.1: the 52px switch box shrinks under a long label in a tight row. */
  lit-material-switch {
    display: grid;
    grid-template-columns: 52px minmax(0, 1fr);
    align-items: center;
  }
  lit-material-switch::part(text) {
    margin-inline-start: var(--space-100);
  }
  lit-material-data-table {
    width: 100%;
  }
  /* Wide content scrolls inside its box, never the page (phones). */
  .scroll {
    overflow-x: auto;
    max-width: 100%;
    padding-bottom: var(--space-50);
  }
  .scroll > lit-material-data-table {
    min-width: max-content;
  }
  .scroll lit-material-clipboard-copy {
    min-width: 18rem;
  }
  .sub {
    display: grid;
    color: var(--md-sys-color-on-surface-variant);
    font-size: var(--text-xs);
    line-height: var(--leading-xs);
  }
  .who {
    display: grid;
  }
  .who .sub {
    overflow-wrap: anywhere;
  }
  dialog-form,
  .dialog-form {
    display: grid;
    gap: var(--space-200);
    min-width: min(360px, 80vw);
    padding-top: var(--space-75);
  }
  .meters {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: var(--space-200) var(--space-400);
  }
  .banner {
    margin-bottom: var(--space-300);
  }
  @media (max-width: 599.98px) {
    h1 {
      font-size: var(--text-2xl);
      line-height: var(--leading-2xl);
    }
  }
`;
