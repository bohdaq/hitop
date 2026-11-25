// A part of the app shell (HitopApp, ../hitop-app.js).
import { IS_TAURI, platform } from "../../platform/index.js";
import { html, nothing } from "lit";
import { icon } from "../../icons.js";
import { mark } from "../../../../site/src/brand/mark.js";
import { unsafeSVG } from "lit/directives/unsafe-svg.js";

export const MainPart = (Base) =>
  class extends Base {
  // --- main -------------------------------------------------------------------------

  renderMain() {
    const s = this.store;
    const { kind } = s.view;
    return kind === "request" ? this.renderPage() : html`${this.backToRequest()}${this.renderPage()}`;
  }

  renderPage() {
    const s = this.store;
    const { kind, collectionId } = s.view;
    const page = platform().pages?.find((p) => p.kind === kind);
    if (page) return page.render(s, this);
    if (kind === "history") {
      return html`<hitop-history .store=${s} .version=${s.version} .app=${this}></hitop-history>`;
    }
    if (kind === "org") {
      return html`<hitop-org .store=${s} .version=${s.version} .app=${this}></hitop-org>`;
    }
    if (kind === "openapi") {
      return html`<hitop-openapi-editor .store=${s} .version=${s.version} .app=${this}></hitop-openapi-editor>`;
    }
    if (kind === "settings") {
      return html`<hitop-app-settings .store=${s} .version=${s.version} .app=${this}></hitop-app-settings>`;
    }
    if (kind === "cookies") {
      return html`<hitop-cookies .store=${s} .version=${s.version} .app=${this}></hitop-cookies>`;
    }
    if (kind === "environments") {
      return html`<hitop-environments .store=${s} .version=${s.version} .app=${this}></hitop-environments>`;
    }
    if (kind === "collections") {
      return html`<hitop-collections .store=${s} .version=${s.version} .app=${this}></hitop-collections>`;
    }
    if (kind === "collection" || kind === "variables") {
      const col = s.collection(collectionId);
      if (!col) return html`<hitop-collections .store=${s} .version=${s.version} .app=${this}></hitop-collections>`;
      return kind === "variables"
        ? html`<hitop-variables .store=${s} .version=${s.version} .collection=${col}></hitop-variables>`
        : html`<hitop-collection .store=${s} .version=${s.version} .app=${this} .collection=${col}></hitop-collection>`;
    }
    return html`${this.renderTabs()}<hitop-request .store=${s} .version=${s.version} .app=${this} .tab=${s.tab}></hitop-request>`;
  }

  render() {
    return html`
      <a class="skip-link" href="#main">Skip to content</a>
      <header class="topbar">
        <lit-material-icon-button class="nav-toggle" aria-label=${this.navOpen ? "Close collections" : "Open collections"} aria-expanded=${String(this.navOpen)} @click=${() => (this.navOpen = !this.navOpen)}>
          ${icon(this.navOpen ? "x" : "list")}
        </lit-material-icon-button>
        ${IS_TAURI
          ? html`<span class="brand">${unsafeSVG(mark())}<span>HITOP</span></span>`
          : html`<a class="brand" href="/" aria-label="HITOP website">${unsafeSVG(mark())}<span>HITOP</span></a>`}
        ${this.crumb()}
        ${this.phoneEnvChip()}
        <span class="topbar-spacer"></span>
        ${this.commandField()}
        <span class="topbar-spacer"></span>
        ${this.tools("in-topbar")}
        ${this.branchPill()}
        ${this.accountButton()}
      </header>
      <div class=${`layout ${this.sideCollapsed() ? "side-collapsed" : ""}`}>
        ${this.renderSidebar()}
        <div class="scrim" ?hidden=${!this.navOpen} @click=${() => (this.navOpen = false)}></div>
        <main id="main" class="main" tabindex="-1">${this.renderMain()}</main>
      </div>
      ${this.renderStatusBar()}
      ${this.renderDialog()} ${this.renderRequestMenu()} ${this.renderCollectionMenu()} ${this.renderFolderMenu()} ${this.renderAccountMenu()} ${this.renderPalette()} ${this.renderAgent()}
      <lit-material-snackbar id="toast" closable>${this.toastAction
        ? html`<lit-material-button slot="action" variant="text" @click=${() => this.toastAction?.run()}>${this.toastAction.label}</lit-material-button>`
        : nothing}</lit-material-snackbar>`;
  }
  };
