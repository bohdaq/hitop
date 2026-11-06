// Comments on a request of a shared collection (sync.js `comments`, `addComment`): end to end
// encrypted like the collection, one file each, synced with it. The request editor's Comments tab.
import { html, nothing } from "lit";
import { icon } from "../icons.js";

export function commentsPanel(view, sync, collectionId, request) {
  const list = sync.comments(collectionId, request);
  const run = async (fn) => {
    view.commentError = null;
    view.commentBusy = true;
    view.requestUpdate();
    try {
      await fn();
    } catch (e) {
      view.commentError = e.message || String(e);
    } finally {
      view.commentBusy = false;
      view.requestUpdate();
    }
  };
  const post = () =>
    run(async () => {
      const field = view.querySelector("#comment-text");
      await sync.addComment(collectionId, request, field?.value ?? "");
      if (field) field.value = "";
    });
  return html`<div class="comments" aria-label="Comments">
    <p class="hint">Seen by the people this collection is shared with, end to end encrypted like the requests. <lit-material-button variant="text" ?disabled=${view.commentBusy} @click=${() => run(() => sync.syncNow(collectionId))}>${icon("arrows-clockwise")} Refresh</lit-material-button></p>
    ${view.commentError ? html`<p class="error-text" role="alert">${view.commentError}</p>` : nothing}
    ${list.length
      ? html`<ol class="sync-list comment-list">
          ${list.map((c) => html`<li class="sync-item">
            <span class="hint"><b>${c.author}</b> · ${c.at ? new Date(c.at).toLocaleString() : ""}</span>
            <p class="comment-text">${c.text}</p>
            ${c.mine ? html`<lit-material-button variant="text" ?disabled=${view.commentBusy} @click=${() => run(() => sync.deleteComment(collectionId, c.path))}>Delete</lit-material-button>` : nothing}
          </li>`)}
        </ol>`
      : html`<p class="hint">No comments yet.</p>`}
    <lit-material-textarea id="comment-text" variant="outlined" label="Add a comment" rows="3"></lit-material-textarea>
    <lit-material-button variant="filled" ?disabled=${view.commentBusy} @click=${post}>${icon("paper-plane-tilt")} Comment</lit-material-button>
  </div>`;
}
