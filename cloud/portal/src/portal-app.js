// <portal-app>: the console frame. A top bar with the organization switcher and the account
// menu, navigation that is a drawer, a rail or a bottom bar depending on the window, the routed
// page and the footer (AGPL section 13: the commit and its source). The sign-in page gets the
// whole window. Real paths (/orgs/<id>/plan); older #/ links are mapped in legacy.js.
import "@lit-material/router";
import "@lit-material/top-app-bar";
import "@lit-material/navigation/adaptive-navigation.js";
import "@lit-material/menu";
import "@lit-material/list";
import "@lit-material/avatar";
import "@lit-material/button";
import { navigate, RouteController } from "@lit-material/router";
import { TaskController } from "@lit-material/task";
import { LitElement, css, html, nothing } from "lit";
import { session } from "./session.js";
import { orgName } from "./format.js";
import { expandIcon, homeIcon, keyIcon, orgIcon, personIcon, workspacesIcon } from "./icons.js";
import "./pages/sign-in-page.js";
import "./pages/home-page.js";
import "./pages/workspaces-page.js";
import "./pages/runs-page.js";
import "./pages/orgs-page.js";
import "./pages/org-page.js";
import "./pages/members-page.js";
import "./pages/plan-page.js";
import "./pages/settings-page.js";
import "./pages/tokens-page.js";
import "./pages/account-page.js";

const guard = ({ path }) => {
  if (session.signedIn) return true;
  session.next = path;
  return "/sign-in";
};

const routes = [
  { path: "/sign-in", title: "Sign in", render: () => html`<sign-in-page></sign-in-page>` },
  { path: "/", guard, title: "Home", render: () => html`<home-page></home-page>` },
  { path: "/workspaces", guard, title: "Workspaces", render: () => html`<workspaces-page></workspaces-page>` },
  { path: "/workspaces/:ws/runs", guard, title: "Runs", render: (p) => html`<runs-page .ws=${p.ws}></runs-page>` },
  { path: "/workspaces/:ws/runs/:run", guard, title: "Run", render: (p) => html`<runs-page .ws=${p.ws} .run=${p.run}></runs-page>` },
  { path: "/orgs", guard, title: "Organizations", render: () => html`<orgs-page></orgs-page>` },
  { path: "/orgs/:org", guard, title: "Organization", render: (p) => html`<org-page .orgId=${p.org}></org-page>` },
  { path: "/orgs/:org/members", guard, title: "Members", render: (p) => html`<members-page .orgId=${p.org}></members-page>` },
  { path: "/orgs/:org/plan", guard, title: "Plan and billing", render: (p) => html`<plan-page .orgId=${p.org}></plan-page>` },
  { path: "/orgs/:org/settings", redirect: "/orgs/:org/settings/general" },
  { path: "/orgs/:org/settings/:tab", guard, title: "Settings", render: (p) => html`<settings-page .orgId=${p.org} .tab=${p.tab}></settings-page>` },
  { path: "/tokens", guard, title: "Access tokens", render: () => html`<tokens-page></tokens-page>` },
  { path: "/account", guard, title: "Account", render: () => html`<account-page></account-page>` },
  { path: "/*", title: "Not found", status: 404, render: (p) => html`<not-found-page .path=${p.wildcard}></not-found-page>` },
];

export class PortalApp extends LitElement {
  static styles = css`
    :host {
      display: grid;
      grid-template: "bar bar" auto "nav main" 1fr "foot foot" auto / auto 1fr;
      min-height: 100vh;
    }
    :host([bare]) {
      display: block;
    }
    lit-material-top-app-bar {
      grid-area: bar;
      border-bottom: 1px solid var(--md-sys-color-outline-variant);
    }
    lit-material-adaptive-navigation {
      grid-area: nav;
    }
    main {
      grid-area: main;
      min-width: 0;
      box-sizing: border-box;
      width: 100%;
      max-width: 1120px;
      padding: var(--space-300);
    }
    footer {
      grid-area: foot;
      display: flex;
      flex-wrap: wrap;
      justify-content: space-between;
      gap: var(--space-200);
      padding: var(--space-200) var(--space-300);
      border-top: 1px solid var(--md-sys-color-outline-variant);
      color: var(--md-sys-color-on-surface-variant);
      font-size: var(--text-xs);
      line-height: var(--leading-xs);
    }
    footer a {
      color: inherit;
    }
    .brand {
      display: inline-flex;
      align-items: center;
      gap: var(--space-75);
      color: inherit;
      text-decoration: none;
      font-weight: 600;
      font-size: var(--text-base);
    }
    .brand img {
      width: 24px;
      height: 24px;
    }
    .org {
      display: inline-flex;
      align-items: center;
      gap: var(--space-50);
      margin-inline-start: var(--space-100);
      padding: 6px var(--space-75) 6px var(--space-100);
      border: 0;
      border-radius: 8px;
      background: none;
      color: inherit;
      font: inherit;
      font-weight: 500;
      cursor: pointer;
      max-width: 40vw;
    }
    .org span {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .org:hover {
      background: var(--md-sys-color-surface-container-high);
    }
    .org:focus-visible,
    .account:focus-visible {
      outline: 2px solid var(--md-sys-color-primary);
      outline-offset: 2px;
    }
    .org svg {
      width: 18px;
      height: 18px;
      flex: none;
    }
    .actions {
      display: flex;
      align-items: center;
      gap: var(--space-50);
      padding-inline-end: var(--space-75);
    }
    .account {
      padding: 4px;
      border: 0;
      border-radius: 50%;
      background: none;
      cursor: pointer;
    }
    .menu-email {
      padding: var(--space-75) var(--space-200) var(--space-50);
      color: var(--md-sys-color-on-surface-variant);
      font-size: var(--text-xs);
      overflow-wrap: anywhere;
    }
    @media (max-width: 599.98px) {
      :host {
        grid-template: "bar" auto "main" 1fr "foot" auto / 1fr;
        padding-bottom: 80px;
      }
      lit-material-adaptive-navigation {
        position: fixed;
        inset: auto 0 0;
        z-index: 1;
      }
      main {
        padding: var(--space-200);
      }
      .brand span {
        display: none;
      }
    }
  `;

  static properties = {
    _tick: { state: true },
  };

  constructor() {
    super();
    this.route = new RouteController(this, () => routes);
    this.orgs = new TaskController(this, {
      task: ([token], signal) => (token ? session.api.get("/v1/orgs", { signal }).then((r) => r.orgs) : Promise.resolve([])),
      args: () => [session.signedIn ? session.account.session : ""],
      key: ([token]) => ["orgs", token],
    });
    this._tick = 0;
    this.onBusy = () => this.setAttribute("aria-busy", session.pending > 0 ? "true" : "false");
    this.onChange = () => (this._tick += 1);
  }

  connectedCallback() {
    super.connectedCallback();
    this.setAttribute("aria-busy", "false");
    session.addEventListener("busy", this.onBusy);
    session.addEventListener("change", this.onChange);
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    session.removeEventListener("busy", this.onBusy);
    session.removeEventListener("change", this.onChange);
  }

  updated() {
    this.toggleAttribute("bare", this.route.current.path === "/sign-in");
  }

  get currentOrg() {
    const orgs = this.orgs.value || [];
    return orgs.find((o) => o.id === session.orgId) || orgs.find((o) => o.personal) || orgs[0] || null;
  }

  render() {
    const outlet = html`<lit-material-router-outlet .routes=${routes} title-template="%s · HITOP Cloud"></lit-material-router-outlet>`;
    const path = this.route.current.path;
    if (path === "/sign-in" || !session.signedIn) return html`${outlet}${this.footer()}`;
    const org = this.currentOrg;
    const destinations = [
      { label: "Home", icon: homeIcon, href: "/" },
      { label: "Workspaces", icon: workspacesIcon, href: "/workspaces" },
      { label: "Organization", icon: orgIcon, href: org ? `/orgs/${encodeURIComponent(org.id)}` : "/orgs" },
      { label: "Access tokens", icon: keyIcon, href: "/tokens" },
      { label: "Account", icon: personIcon, href: "/account" },
    ];
    const selected = destinations.findIndex((d) => (d.href === "/" ? path === "/" : path.startsWith(d.href.startsWith("/orgs") ? "/orgs" : d.href)));
    const initials = (session.email || "?").slice(0, 2).toUpperCase();
    return html`
      <lit-material-top-app-bar>
        <a class="brand" href="/" aria-label="HITOP Cloud home"><img src="https://hitop.rws8.tech/favicon.svg" alt="" width="24" height="24" /><span>HITOP Cloud</span></a>
        <button class="org" aria-label="Switch organization" aria-haspopup="true" @click=${(e) => this.renderRoot.querySelector("#org-menu").show(e.currentTarget)}>
          <span>${org ? orgName(org) : "Organizations"}</span>${expandIcon}
        </button>
        <div slot="trailing" class="actions">
          <button class="account" aria-label="Account menu" aria-haspopup="true" @click=${(e) => this.renderRoot.querySelector("#account-menu").show(e.currentTarget)}>
            <lit-material-avatar initials=${initials} alt=""></lit-material-avatar>
          </button>
        </div>
      </lit-material-top-app-bar>
      <lit-material-menu id="org-menu" aria-label="Organizations">
        ${(this.orgs.value || []).map(
          (o) => html`<lit-material-list-item interactive ?selected=${org?.id === o.id} @click=${() => this.pickOrg(o)}>${orgName(o)}</lit-material-list-item>`,
        )}
        <lit-material-list-item interactive @click=${() => navigate("/orgs")}>All organizations</lit-material-list-item>
        <lit-material-list-item interactive @click=${() => navigate("/orgs?new=1")}>New organization</lit-material-list-item>
      </lit-material-menu>
      <lit-material-menu id="account-menu" aria-label="Account">
        <div class="menu-email">${session.email}${session.hosted ? nothing : html`<br />on ${session.server}`}</div>
        <lit-material-list-item interactive @click=${() => navigate("/account")}>Account</lit-material-list-item>
        <lit-material-list-item interactive @click=${() => navigate("/tokens")}>Access tokens</lit-material-list-item>
        <lit-material-list-item interactive @click=${this.signOut}>Sign out</lit-material-list-item>
      </lit-material-menu>
      <lit-material-adaptive-navigation label="Portal" .destinations=${destinations} .selected=${selected}></lit-material-adaptive-navigation>
      <main id="main">${outlet}</main>
      ${this.footer()}
    `;
  }

  footer() {
    const commit = typeof __HITOP_COMMIT__ === "string" ? __HITOP_COMMIT__ : "main";
    return html`<footer>
      <span>HITOP Cloud portal, <a href="https://github.com/bohdaq/hitop/tree/main/cloud" rel="noopener">source (AGPL-3.0-or-later)</a>, commit <a id="portal-commit" href="https://github.com/bohdaq/hitop/tree/${commit}/cloud/portal" rel="noopener">${commit}</a>.</span>
      <a href="https://hitop.rws8.tech/privacy.html">Privacy</a>
    </footer>`;
  }

  pickOrg(o) {
    session.setOrg(o.id);
    navigate(`/orgs/${encodeURIComponent(o.id)}`);
  }

  signOut = async () => {
    await session.api.call("POST", "/v1/auth/logout").catch(() => {});
    session.signOut();
    navigate("/sign-in");
  };
}

customElements.define("portal-app", PortalApp);

export class NotFoundPage extends LitElement {
  static properties = { path: {} };
  createRenderRoot() {
    return this;
  }
  render() {
    return html`<h1>That page doesn't exist</h1><p>Nothing at <code>/${this.path || ""}</code>. <a href="/">Go home</a></p>`;
  }
}
customElements.define("not-found-page", NotFoundPage);
