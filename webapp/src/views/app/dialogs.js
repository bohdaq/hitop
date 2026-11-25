// A part of the app shell (HitopApp, ../hitop-app.js).
import { FIT_DIALOGS, WIDE_DIALOGS, codeDialog, compareDialog, refreshCode } from "../tools-dialogs.js";
import { IS_TAURI, platform } from "../../platform/index.js";
import { NEW_COLLECTION, inviteEmails, readableError } from "./shared.js";
import { OPENAPI_DIALOGS, openapiDialog, submitOpenapi } from "../openapi-dialogs.js";
import { UNTITLED, isBlankRequest } from "../../new-request.js";
import { confirm } from "@lit-material/confirm";
import { folderOf, slugName } from "../../collection-tree.js";
import { historyDialog } from "../history-dialog.js";
import { html, nothing } from "lit";
import { icon } from "../../icons.js";
import { isCurl, parseCurl } from "../../curl.js";
import { issueText } from "../../pm.js";
import { loadFormat } from "../../wasm/format.js";
import { planLabel, storeBuild } from "../../store-mode.js";
import { shortcutList } from "../shortcut-list.js";
import { signInDialog } from "../sign-in-dialog.js";
import { syncClient } from "../sync-view.js";

export const DialogsPart = (Base) =>
  class extends Base {
  // --- dialogs ----------------------------------------------------------------

  async openDialog(kind, data = {}) {
    this.navOpen = false;
    // A dialog closed just before (closeDialog, then this) fires its close event later, and that
    // clears this.dialog: wait for it first, or it takes the new dialog away (library e2e flake).
    if (this.dialogClosing) {
      await Promise.race([this.dialogClosing, new Promise((r) => setTimeout(r, 1000))]);
      this.dialogClosing = this.dialogClosed = null; // also when it never came (the element was replaced)
    }
    this.dialog = { kind, ...data, opened: (this.dialogsOpened = (this.dialogsOpened ?? 0) + 1) };
    if (kind === "code") refreshCode(this);
    await this.updateComplete;
    const dlg = this.querySelector("#dialog");
    dlg?.show();
    // The dialog focuses itself as it opens; move focus to the first field after.
    await dlg?.updateComplete;
    requestAnimationFrame(() => dlg?.querySelector("[autofocus]")?.focus());
  }

  /** The dialog `d` was opened as is still the one on screen (an answer that comes late changes nothing). */
  stillOpen(d) {
    return Boolean(this.dialog) && this.dialog.opened === d.opened;
  }

  // The form is novalidate: empty required fields get their message on the
  // field, not in the browser's bubble.
  validateDialog() {
    let first = null;
    for (const el of this.querySelectorAll("#dialog-form [required]")) {
      const empty = !String(el.value ?? "").trim();
      el.error = empty;
      const label = el.getAttribute("label") ?? "";
      el.errorText = empty ? `Enter the ${label.charAt(0).toLowerCase()}${label.slice(1)}` : "";
      if (empty) first ??= el;
    }
    first?.focus();
    return !first;
  }

  clearFieldError(el) {
    if (!el?.error) return;
    el.error = false;
    el.errorText = "";
  }

  closeDialog() {
    const dlg = this.querySelector("#dialog");
    if (!dlg?.open) return;
    this.dialogClosing ??= new Promise((resolve) => (this.dialogClosed = resolve));
    dlg.close();
  }

  // The dialog's own close event (not one bubbling from a menu inside it).
  onDialogClose(e) {
    if (e.target !== e.currentTarget) return;
    this.dialog = null;
    this.dialogClosed?.();
    this.dialogClosing = this.dialogClosed = null;
  }

  field(id) {
    return this.querySelector(`#dialog #${id}`)?.value ?? "";
  }

  // The fields' inputs live in shadow DOM, so Enter doesn't submit the form
  // by itself. Enter in a textarea still adds a line.
  dialogKey(e) {
    if (e.key !== "Enter" || e.shiftKey || e.isComposing) return;
    // The Sign in dialog's buttons and links take Enter themselves; only its fields submit.
    if (this.dialog?.kind === "sign-in" && !e.composedPath().some((n) => n.localName === "lit-material-text-field")) return;
    if (e.composedPath().some((n) => n.localName === "textarea" || n.localName === "lit-material-select")) return;
    e.preventDefault();
    e.currentTarget.requestSubmit();
  }

  submitDialog(e) {
    e.preventDefault();
    const s = this.store;
    const d = this.dialog;
    // Enter can reach the form twice (the field's own implicit submission and
    // dialogKey); act once per opening.
    if (d?.kind === "share") {
      // Enter in a field: only the GitHub tab's form has one thing to do.
      if (d.tab === "github") this.querySelector("hitop-git-panel")?.submit();
      return;
    }
    if (d?.kind === "ai" || d?.kind === "request-history") return;
    if (d?.kind === "sign-in") {
      // Enter in Work email / Server: Continue with SSO; in the code field: Sign in.
      this.querySelector("#dialog hitop-sign-in")?.submitStep();
      return;
    }
    if (d?.kind === "join") {
      [...this.querySelectorAll("#dialog hitop-sync lit-material-button")].find((b) => b.textContent.includes("Accept invite"))?.click();
      return;
    }
    if (!d || d.submitted) return;
    if (d.kind === "quick-open") {
      const { results, active } = this.quickDialog(d);
      this.quickPick(results[active]);
      return;
    }
    if (!this.validateDialog()) return;
    d.submitted = true;
    if (OPENAPI_DIALOGS.includes(d.kind)) {
      submitOpenapi(this, d);
      return;
    }
    if (d.kind === "new-org" || d.kind === "org-invite") {
      this.submitOrg(d);
      return;
    }
    if (d.kind === "add-collection") {
      // The new collection opens, so the next step (its "New request") is on screen.
      const col = s.addCollection(this.field("name"));
      if (col) this.go("collection", col.id);
    }
    if (d.kind === "new-folder") this.addFolder(d.collectionId, d.parent, this.field("name"));
    if (d.kind === "rename-folder") {
      const newPath = s.renameFolder(d.collectionId, d.path, this.field("name"));
      if (newPath && newPath !== d.path) this.folderRenamed(d.collectionId, d.path, newPath);
    }
    if (d.kind === "move-request") {
      const [to, folder = ""] = String(this.field("target")).split("|");
      const target = s.collections.find((c) => String(c.id) === to);
      if (target && s.moveRequestTo(d.collectionId, d.request.id, target.id, folder)) {
        this.expand(target.id, folder);
        this.toast(`Moved "${d.request.name}" to ${folder ? `${target.name} › ${folder}` : target.name}`);
      }
    }
    if (d.kind === "add-environment") s.addEnvironment(this.field("name"));
    if (d.kind === "rename-environment") s.renameEnvironment(d.id, this.field("name"));
    if (d.kind === "import-environments") {
      try {
        s.importEnvironments(this.field("json"));
      } catch (err) {
        this.dialog = { ...d, submitted: false, error: readableError(err, "an environment file") };
        return;
      }
    }
    if (d.kind === "rename-collection") s.renameCollection(d.id, this.field("name"));
    if (d.kind === "rename-tab") s.renameTab(d.index, this.field("name"));
    if (d.kind === "rename-request") s.renameRequest(d.collectionId, d.request.id, this.field("name"));
    if (d.kind === "save-example") s.saveExample(this.field("name"));
    if (d.kind === "save") {
      // "New collection…" in the picker: made here, the request goes straight into it.
      const picked = String(this.field("collection"));
      const target = picked === NEW_COLLECTION ? s.addCollection(this.field("new-collection")) : s.collections.find((c) => String(c.id) === picked);
      if (!target) {
        this.dialog = { ...d, submitted: false };
        return;
      }
      s.saveRequest(this.field("name"), target.id, target.id === d.collectionId ? d.folder || "" : "");
      this.expand(target.id, target.id === d.collectionId ? d.folder || "" : "");
    }
    if (d.kind === "import") {
      // A Postman import may come back with things to check: show them. An
      // OpenAPI spec is added as a new collection, with its report.
      const text = this.field("json");
      // UX2 (2): a curl command is one new request in the collection on screen.
      if (isCurl(text)) {
        let parsed;
        try {
          parsed = parseCurl(text);
        } catch (err) {
          this.dialog = { ...d, submitted: false, error: `That curl command can't be read: ${err.message}` };
          return;
        }
        this.closeDialog();
        this.newRequest();
        this.applyCurl(parsed, true);
        return;
      }
      const replace = d.mode === "replace";
      const collections = async () => {
        // A file that can't be read says so before anything is asked.
        if (!(await loadFormat()).convertImport(text)) JSON.parse(text.replace(/^\uFEFF/, ""));
        // "Replace all" deletes every collection: ask first when that deletes something.
        if (replace && !(await this.confirmReplace())) return { cancelled: true };
        return { report: await s.importJson(text, { replace }) };
      };
      s.isOpenapi(text)
        .then(async (openapi) => (openapi ? { openapi: await s.importOpenapi({ text }) } : collections()))
        .then(
        ({ openapi, report, cancelled }) => {
          if (!this.stillOpen(d)) return;
          if (cancelled) this.dialog = { ...d, submitted: false };
          else if (openapi) this.dialog = { kind: "openapi-report", report: openapi, opened: d.opened };
          else if (report?.issues.length) this.dialog = { kind: "import-report", report, opened: d.opened };
          else this.closeDialog();
        },
        // Closed meanwhile: said as a notice instead.
        (err) => (this.stillOpen(d) ? (this.dialog = { ...d, submitted: false, error: readableError(err, "a collection file") }) : this.toast(readableError(err, "a collection file"))),
      );
      return;
    }
    this.closeDialog();
  }

  async confirmReplace() {
    const stake = this.store.collectionsAtStake();
    if (!stake.length) return true;
    const list = stake.map((c) => `${c.name} (${c.requests} request${c.requests === 1 ? "" : "s"})`).join(", ");
    return confirm({
      headline: "Replace your collections?",
      message: `Importing deletes ${stake.length === 1 ? "your collection" : `all ${stake.length} of your collections`}: ${list}. Export first if you want a copy.`,
      confirmLabel: "Replace",
      destructive: true,
    });
  }

  // The file picked (or dropped) in the Import dialog fills its text area.
  async importFile(file) {
    if (!file) return;
    const field = this.querySelector("#dialog #json");
    const d = this.dialog;
    if (!field || !d) return;
    try {
      const text = await file.text();
      if (!this.stillOpen(d)) return;
      field.value = text;
      this.clearFieldError(field);
      this.dialog = { ...this.dialog, file: file.name, error: null };
    } catch (e) {
      if (this.stillOpen(d)) this.dialog = { ...this.dialog, error: `Couldn't read ${file.name}: ${e?.message ?? e}` };
    }
  }

  // Closing a tab whose edits aren't saved asks first.
  async closeTab(index) {
    const s = this.store;
    const tab = s.tabs[index];
    if (!tab || s.tabs.length < 2) return;
    if (s.isDirty(tab) || s.hasDraftContent(tab)) {
      const saved = s.loadedRequest(tab);
      const choice = await this.confirmDiscard({
        headline: `Discard changes to ${tab.title}?`,
        message: !saved
          ? "This request isn't saved in a collection."
          : tab.autoName && isBlankRequest(saved) && saved.name === UNTITLED
            ? `This new request was never saved, so it comes out of ${s.collection(tab.loadedCollectionId)?.name ?? "its collection"} again.`
            : "The tab has edits that aren't saved; the saved request stays as it was.",
        canSave: Boolean(saved),
      });
      if (choice === "cancel") return;
      if (choice === "save") return this.saveAndClose(s.tabs.indexOf(tab));
    }
    s.closeTab(s.tabs.indexOf(tab));
  }

  // UX2: closing a tab with edits: Discard, Cancel, or (a saved request) Save and close.
  // Resolves "discard" | "save" | "cancel".
  async confirmDiscard({ headline, message, canSave }) {
    const el = new (customElements.get("lit-material-confirm-dialog"))();
    el.headline = headline;
    el.confirmLabel = "Discard";
    el.destructive = true;
    let saving = false;
    const p = document.createElement("p");
    p.textContent = message;
    el.append(p);
    if (canSave) {
      const save = document.createElement("lit-material-button");
      save.setAttribute("variant", "outlined");
      save.className = "discard-save";
      save.textContent = "Save and close";
      save.addEventListener("click", () => ((saving = true), el.close(false)));
      el.append(save);
    }
    document.body.append(el);
    try {
      const ok = await el.show();
      return ok ? "discard" : saving ? "save" : "cancel";
    } finally {
      el.remove();
    }
  }

  saveAndClose(index) {
    const s = this.store;
    const tab = s.tabs[index];
    if (!tab || !s.loadedRequest(tab)) return;
    s.selectTab(index);
    s.saveRequest(s.savedName(tab), tab.loadedCollectionId);
    s.closeTab(index);
  }

  // Save: a saved request is overwritten in place (a new one under its automatic name), a draft
  // asks where (even without a URL yet: a named, empty request in a collection is a fine start).
  saveCurrent() {
    const s = this.store;
    const request = s.loadedRequest();
    if (request) s.saveRequest(s.savedName(s.tab, request), s.tab.loadedCollectionId);
    else if (s.tab) this.openDialog("save", s.saveTarget());
  }

  async deleteCollection(collection) {
    const folder = this.store.folderOf(collection.id);
    if (folder) {
      const ok = await confirm({
        headline: `Close ${collection.name}?`,
        message: `It leaves HITOP. The folder ${folder.label} and its files stay where they are; open it again any time.`,
        confirmLabel: "Close",
      });
      if (ok) this.store.deleteCollection(collection.id);
      return;
    }
    const n = collection.requests?.length ?? 0;
    const ok = await confirm({
      headline: `Delete ${collection.name}?`,
      message: `Its ${n} request${n === 1 ? "" : "s"} and variables are deleted too. Export first if you want a copy.`,
      confirmLabel: "Delete",
      destructive: true,
    });
    if (ok) this.store.deleteCollection(collection.id);
  }

  async deleteRequest(request, collectionId) {
    const ok = await confirm({ headline: `Delete ${request.name}?`, message: "The request is removed from its collection.", confirmLabel: "Delete", destructive: true });
    if (ok) this.store.deleteRequest(request, collectionId);
  }

  async deleteEnvironment(env) {
    const ok = await confirm({ headline: `Delete ${env.name}?`, message: "Its variables are deleted too.", confirmLabel: "Delete", destructive: true });
    if (ok) this.store.deleteEnvironment(env.id);
  }

  async clearHistory() {
    const ok = await confirm({ headline: "Clear history?", message: "All saved history entries are removed.", confirmLabel: "Clear", destructive: true });
    if (ok) this.store.clearHistory();
  }

  renderDialog() {
    const d = this.dialog;
    if (!d) return html`<lit-material-dialog id="dialog"></lit-material-dialog>`;
    const s = this.store;
    const actions = (label, destructive = false) => html`
      <div slot="actions" class="dialog-actions">
        <lit-material-button variant="text" @click=${() => this.closeDialog()}>Cancel</lit-material-button>
        <lit-material-button variant="filled" type="submit" form="dialog-form" class=${destructive ? "destructive" : ""}>${label}</lit-material-button>
      </div>`;
    let headline = "";
    let body = nothing;
    let footer = nothing;
    switch (d.kind) {
      case "new-org":
        headline = "New organization";
        body = html`<p>A shared space for your team's collections, environments and runs. Your personal space stays private. You own it; invite people next.</p>
          <lit-material-text-field id="name" variant="outlined" label="Organization name" required autofocus ?error=${Boolean(d.error)} error-text=${d.error ?? ""}></lit-material-text-field>
          ${storeBuild()
            ? nothing
            : html`<fieldset class="org-start">
            <legend>Start on</legend>
            <label class="org-choice"><input type="radio" name="org-start" value="trial" checked /> <span><b>Team trial</b>, free for 14 days, no card: unlimited shared collections, 5 free guest viewers, mocks and monitors.</span></label>
            <label class="org-choice"><input type="radio" name="org-start" value="free" /> <span><b>Free</b>: 3 members, 3 shared collections, 100 MB.</span></label>
          </fieldset>`}`;
        footer = actions("Create and invite people");
        break;
      case "org-invite": {
        const usage = d.seats;
        const seatsMax = usage?.limits?.seats;
        const seatsUsed = usage?.usage?.seats ?? 0;
        const pending = inviteEmails(d.draft ?? "");
        const domain = (s.syncClient?.account?.()?.email ?? "").split("@")[1] ?? "";
        const outside = domain ? pending.filter((e) => !e.endsWith(`@${domain}`)) : [];
        headline = `Invite people to ${s.currentOrgInfo?.name ?? "the organization"}`;
        body = html`<p>They get an email with a link; once they sign in with that address they are members${d.added?.length ? html`. Invited so far: ${d.added.join(", ")}` : ""}.</p>
          <lit-material-textarea id="email" variant="outlined" rows="2" label="Email addresses (comma or space separated)" required autofocus .value=${d.draft ?? ""} ?error=${Boolean(d.error)} error-text=${d.error ?? ""}
            @input=${(e) => (this.dialog = { ...this.dialog, draft: e.target.value })}></lit-material-textarea>
          ${outside.length ? html`<p class="hint warn-text">${outside.join(", ")} ${outside.length === 1 ? "is" : "are"} outside ${domain}: a guest viewer (read only${storeBuild() ? "" : ", free on Team"}) may fit better, from a collection's Share dialog.</p>` : nothing}
          <lit-material-select id="role" variant="outlined" label="Role" .value=${"member"}>
            <lit-material-select-option value="member">Member: edit and run</lit-material-select-option>
            <lit-material-select-option value="admin">Admin: also invite and manage the plan</lit-material-select-option>
          </lit-material-select>
          ${seatsMax != null
            ? html`<p class=${`hint tabular ${seatsUsed + pending.length > seatsMax ? "warn-text" : ""}`} role="status">Seats after these invites: ${seatsUsed + pending.length} of ${seatsMax}${seatsUsed + pending.length > seatsMax ? (storeBuild() ? ". That is more seats than the plan has." : ". Add seats on Plan & billing first.") : ""}</p>`
            : nothing}`;
        footer = actions(pending.length > 1 ? `Send ${pending.length} invites` : "Invite");
        break;
      }
      case "limit": {
        const l = d.limit;
        headline = l.headline;
        body = html`<p>${l.message}</p>
          <ul class="limit-list">
            <li class="ok-text">${icon("check-circle")} What you have keeps syncing and running</li>
            <li class="ok-text">${icon("check-circle")} Everything local, the CLI and CI keep working</li>
            <li class="warn-text">${icon("warning-circle")} ${storeBuild() ? "New ones need a free slot" : "New ones need a bigger plan or a free slot"}</li>
          </ul>
          <hitop-limit-note bare .limit=${l} .store=${s} .client=${s.library?.client ?? syncClient(s)}></hitop-limit-note>`;
        footer = html`<div slot="actions" class="dialog-actions">
          <lit-material-button variant="text" @click=${() => this.closeDialog()}>Close</lit-material-button>
          ${d.admin
            ? html`<lit-material-button variant="filled" @click=${() => (this.closeDialog(), this.openOrg("billing"))}>${planLabel()}</lit-material-button>`
            : html`<lit-material-button variant="filled" @click=${() => (this.closeDialog(), this.openOrg("members"))}>${storeBuild() ? "See members" : "See who can upgrade"}</lit-material-button>`}
        </div>`;
        break;
      }
      case "add-collection":
        headline = "New collection";
        body = html`<lit-material-text-field id="name" variant="outlined" label="Collection name" required autofocus></lit-material-text-field>`;
        footer = actions("Create");
        break;
      case "add-environment":
        headline = "New environment";
        body = html`<lit-material-text-field id="name" variant="outlined" label="Environment name" placeholder="Staging" required autofocus></lit-material-text-field>`;
        footer = actions("Create");
        break;
      case "rename-environment":
        headline = "Rename environment";
        body = html`<lit-material-text-field id="name" variant="outlined" label="Environment name" .value=${d.name} required autofocus></lit-material-text-field>`;
        footer = actions("Rename");
        break;
      case "import-environments":
        headline = "Import environments";
        body = html`
          <p class="dialog-note">Paste a HITOP environments export or a Postman environment. They are added to the ones you have.</p>
          <lit-material-textarea id="json" variant="outlined" label="Environment JSON" rows="10" required autofocus
            ?error=${Boolean(d.error)} error-text=${d.error ?? ""}></lit-material-textarea>`;
        footer = actions("Import");
        break;
      case "rename-collection":
        headline = "Rename collection";
        body = html`<lit-material-text-field id="name" variant="outlined" label="Collection name" .value=${d.name} required autofocus></lit-material-text-field>`;
        footer = actions("Rename");
        break;
      case "rename-request":
      case "rename-tab":
        headline = "Rename request";
        body = html`<lit-material-text-field id="name" variant="outlined" label="Request name" .value=${d.name} required autofocus></lit-material-text-field>`;
        footer = actions("Rename");
        break;
      case "save": {
        // A draft with no collection to go to (none yet) starts on "New collection…".
        const picked = d.picked ?? (s.visibleCollections.some((c) => c.id === d.collectionId) ? String(d.collectionId) : NEW_COLLECTION);
        headline = d.overwrite ? "Save request" : "Save to a collection";
        body = html`
          <lit-material-text-field id="name" variant="outlined" label="Request name" .value=${d.name} required autofocus></lit-material-text-field>
          <lit-material-select id="collection" variant="outlined" label="Collection" .value=${picked} required
            @change=${(e) => (this.dialog = { ...this.dialog, picked: e.target.value })}>
            ${s.visibleCollections.map((c) => html`<lit-material-select-option value=${String(c.id)}>${c.name}</lit-material-select-option>`)}
            <lit-material-select-option value=${NEW_COLLECTION}>${icon("plus")} New collection…</lit-material-select-option>
          </lit-material-select>
          ${picked === NEW_COLLECTION
            ? html`<lit-material-text-field id="new-collection" variant="outlined" label="Collection name" placeholder="My API" required></lit-material-text-field>`
            : nothing}
          ${d.folder && picked === String(d.collectionId) ? html`<p class="hint">${icon("folder-simple")} In the folder ${d.folder}</p>` : nothing}`;
        footer = actions("Save");
        break;
      }
      case "move-request": {
        const targets = s.moveTargets();
        const here = `${d.collectionId}|${folderOf(d.request)}`;
        headline = `Move ${d.request.name}`;
        body = html`
          <lit-material-select id="target" variant="outlined" label="To" .value=${here} required autofocus>
            ${targets.map((t) => html`<lit-material-select-option value=${`${t.collectionId}|${t.folder}`}>${t.label}</lit-material-select-option>`)}
          </lit-material-select>
          <p class="hint">Collections and their folders. You can also drag a request onto a collection or folder in the sidebar.</p>`;
        footer = actions("Move");
        break;
      }
      case "new-folder":
        headline = d.parent ? `New folder in ${d.parent}` : "New folder";
        body = html`
          <lit-material-text-field id="name" variant="outlined" label="Folder name" placeholder="users" required autofocus></lit-material-text-field>
          <p class="hint">A folder is the path of its requests' files (<code>requests/${d.parent ? `${d.parent}/` : ""}&lt;folder&gt;/&lt;request&gt;.yaml</code>); it stays once a request is saved in it.</p>`;
        footer = actions("Create");
        break;
      case "rename-folder":
        headline = "Rename folder";
        body = html`<lit-material-text-field id="name" variant="outlined" label="Folder name" .value=${d.name} required autofocus></lit-material-text-field>`;
        footer = actions("Rename");
        break;
      case "import": {
        headline = "Import collections";
        const replace = d.mode === "replace";
        const mode = (value, label, help) => html`<label class="import-mode-option">
          <input type="radio" name="import-mode" value=${value} .checked=${(d.mode ?? "add") === value} @change=${() => (this.dialog = { ...this.dialog, mode: value })} />
          <span><strong>${label}</strong><span class="hint">${help}</span></span>
        </label>`;
        body = html`
          <p class="dialog-note">Choose a file, drop one here or paste a HITOP, Postman v2.1, Insomnia, Bruno or HAR file, an OpenAPI 3 spec (YAML or JSON, always added as a new collection) or a curl command (one new request).</p>
          <fieldset class="import-mode">
            <legend>Imported collections</legend>
            ${mode("add", "Add to my collections", "A name you already have gets (2) added.")}
            ${mode("replace", "Replace all my collections", "Deletes the ones you have now (you're asked first).")}
          </fieldset>
          <label class="file-pick">
            <input type="file" class="visually-hidden" accept=".json,.yaml,.yml,.har,application/json,application/yaml,text/yaml"
              @change=${(e) => this.importFile(e.target.files?.[0])} />
            <span class="file-button">${icon("file-arrow-up")} Choose a file</span>
            <span class="file-name">${d.file ?? "No file chosen"}</span>
          </label>
          <div class="drop-zone" @dragover=${(e) => e.preventDefault()} @drop=${(e) => (e.preventDefault(), this.importFile(e.dataTransfer?.files?.[0]))}>
            <lit-material-textarea id="json" variant="outlined" label="Collection JSON, OpenAPI spec or curl command" placeholder="Paste a HITOP, Postman, Insomnia, Bruno or HAR file, an OpenAPI spec, or a curl command" rows="10" required autofocus
              ?error=${Boolean(d.error)} error-text=${d.error ?? ""}></lit-material-textarea>
          </div>
          <lit-material-button variant="text" class="dialog-link" @click=${() => this.openDialog("openapi-import")}>${icon("globe")} Load an OpenAPI spec from a URL</lit-material-button>`;
        footer = replace ? actions("Replace and import", true) : actions("Import");
        break;
      }
      case "import-report": {
        const { name, issues } = d.report;
        const scripts = issues.filter((i) => i.line).length;
        headline = "Check after import";
        body = html`
          <p class="dialog-note">
            ${name} is imported. ${issues.length === 1 ? "One thing" : `${issues.length} things`} didn't come over as they were${scripts
              ? html`; ${scripts === 1 ? "one script line uses" : `${scripts} script lines use`} Postman calls HITOP doesn't run yet`
              : ""}.
          </p>
          <ul class="import-report">
            ${issues.map((i) => html`<li><span>${issueText(i)}</span>${i.text ? html`<code>${i.text}</code>` : nothing}</li>`)}
          </ul>`;
        // UX2: after the import there is nothing to cancel.
        footer = html`<div slot="actions" class="dialog-actions"><lit-material-button variant="filled" type="submit" form="dialog-form">Done</lit-material-button></div>`;
        break;
      }
      case "export": {
        headline = "Export collections";
        // UX2: one collection or all of them.
        const only = d.only && d.only !== "all" ? d.only : null;
        const json = s.exportJson(this.exportFormat, only);
        const picked = only ? s.collection(Number(only)) ?? s.collections.find((c) => String(c.id) === String(only)) : null;
        const base = picked ? slugName(picked.name) : "hitop-collections";
        const name = this.exportFormat === "postman" ? `${picked ? base : "hitop"}-postman.json` : `${base}.json`;
        body = html`
          <lit-material-select id="export-collection" variant="outlined" label="Collections" .value=${d.only ?? "all"} @change=${(e) => (this.dialog = { ...this.dialog, only: e.target.value })}>
            <lit-material-select-option value="all">All collections (${s.collections.length})</lit-material-select-option>
            ${s.visibleCollections.map((c) => html`<lit-material-select-option value=${String(c.id)}>${c.name}</lit-material-select-option>`)}
          </lit-material-select>
          <lit-material-select variant="outlined" label="Format" .value=${this.exportFormat} @change=${(e) => (this.exportFormat = e.target.value)}>
            <lit-material-select-option value="hitop">HITOP</lit-material-select-option>
            <lit-material-select-option value="postman">Postman v2.1</lit-material-select-option>
          </lit-material-select>
          <lit-material-code-block label=${name} expandable>${json}</lit-material-code-block>`;
        footer = html`
          <div slot="actions" class="dialog-actions">
            <lit-material-button variant="text" @click=${() => this.closeDialog()}>Close</lit-material-button>
            <lit-material-button variant="filled" @click=${() => this.download(name, json)}>${icon("file-arrow-down")} Download</lit-material-button>
          </div>`;
        break;
      }
      case "save-example":
        headline = "Save as example";
        body = html`
          <p class="dialog-note">The response is saved with ${d.request}, as an example to compare later (and in its folder file).</p>
          <lit-material-text-field id="name" variant="outlined" label="Example name" .value=${d.name} required autofocus></lit-material-text-field>`;
        footer = actions("Save");
        break;
      case "openapi-import":
      case "openapi-report":
      case "openapi-sync":
        ({ headline, body, footer } = openapiDialog(this, d));
        break;
      case "code":
      case "compare":
        ({ headline, body, footer } = (d.kind === "code" ? codeDialog : compareDialog)(this, d));
        break;
      case "request-history":
        ({ headline, body, footer } = historyDialog(this, d));
        break;
      case "quick-open": {
        headline = "Quick open";
        ({ body } = this.quickDialog(d));
        footer = html`<div slot="actions" class="dialog-actions quick-actions">
          <span class="hint">↑ ↓ to choose, Enter to open, Esc to close</span>
          <lit-material-button variant="text" @click=${() => this.closeDialog()}>Close</lit-material-button>
        </div>`;
        break;
      }
      case "share": {
        const c = s.collections.find((x) => x.id === d.collectionId);
        headline = `Share ${c?.name ?? "the collection"}`;
        body = c ? this.shareBody(c, d.tab) : nothing;
        footer = html`
          <div slot="actions" class="dialog-actions">
            <lit-material-button variant="text" @click=${() => this.closeDialog()}>Close</lit-material-button>
          </div>`;
        break;
      }
      case "join":
        headline = "Join a shared collection";
        body = html`<p>Paste the invite code someone sent you.</p><hitop-sync .store=${s} .version=${s.version} .app=${this} .join=${true}></hitop-sync>`;
        footer = html`
          <div slot="actions" class="dialog-actions">
            <lit-material-button variant="text" @click=${() => this.closeDialog()}>Close</lit-material-button>
          </div>`;
        break;
      case "sign-in":
        ({ headline, body, footer } = signInDialog(this, d));
        break;
      case "shortcuts":
        headline = "Keyboard shortcuts";
        body = html`${shortcutList()}<p class="dialog-note">They're on the Settings page too.</p>`;
        footer = html`
          <div slot="actions" class="dialog-actions">
            <lit-material-button variant="filled" @click=${() => this.closeDialog()}>Close</lit-material-button>
          </div>`;
        break;
    }
    return html`
      <lit-material-dialog id="dialog" class=${WIDE_DIALOGS.includes(d.kind) || d.kind === "quick-open" || d.kind === "ai" ? "wide" : FIT_DIALOGS.includes(d.kind) ? "fit" : d.kind === "sign-in" ? "sign-in-dialog" : ""} @close=${(e) => this.onDialogClose(e)}>
        <span slot="headline">${headline}</span>
        <form id="dialog-form" class="dialog-form" novalidate @submit=${(e) => this.submitDialog(e)} @input=${(e) => this.clearFieldError(e.target)} @keydown=${(e) => this.dialogKey(e)}>${body}</form>
        ${footer}
      </lit-material-dialog>`;
  }

  // The browser downloads the file; the desktop asks where to save it.
  async download(name, text, type = "application/json") {
    try {
      if (await platform().saveFile(name, text, type)) this.toast(IS_TAURI ? `Saved ${name}` : `Downloaded ${name}`);
    } catch (e) {
      this.toast(`Couldn't save ${name}: ${e?.message ?? e}`);
    }
  }
  };
