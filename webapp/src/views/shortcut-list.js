// The keyboard shortcuts as a list: the ? dialog and the Settings page.
import { html, nothing } from "lit";
import { SHORTCUTS } from "../shortcuts.js";

export const shortcutList = () => html`<dl class="shortcut-list">
  ${SHORTCUTS.map(
    ([keys, what]) =>
      html`<dt>${keys.map((k, i) => html`${i ? html`<span class="key-then">${what.startsWith("Leave") ? "then" : "+"}</span>` : nothing}<kbd>${k}</kbd>`)}</dt><dd>${what}</dd>`,
  )}
</dl>`;
