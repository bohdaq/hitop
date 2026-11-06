import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "@lit-material/tokens/css/index.css";
import "../../site/src/theme.css";
import "./app.css";

import "@lit-material/alert";
import "@lit-material/button";
import "@lit-material/checkbox";
import "@lit-material/code-block";
import "@lit-material/color-scheme";
import "@lit-material/confirm";
import "@lit-material/dialog";
import "@lit-material/empty-state";
import "@lit-material/icon-button";
import "@lit-material/key-value";
import "@lit-material/list";
import "@lit-material/menu";
import "@lit-material/select";
import "@lit-material/sortable";
import "@lit-material/snackbar";
import "@lit-material/status";
import "@lit-material/switch";
import "@lit-material/tabs";
import "@lit-material/text-field";

import { initPlatform } from "./platform/index.js";

// Storage loads before the app starts (the desktop reads it from Rust).
const p = await initPlatform();
// hitop sync (web and desktop): shared collections kept in app storage, and the Account page (top bar).
const { withSyncFolders, SyncClient } = await import("./sync.js");
const { syncPage } = await import("./views/sync-view.js");
p.folders = withSyncFolders(p.folders);
p.pages = [...(p.pages ?? []), syncPage];
// HITOP Cloud's personal library (library.js): signed in, collections, environments and globals
// are pulled into storage before the store reads it, and pushed after each save.
const { Library } = await import("./library.js");
const library = new Library(new SyncClient(null));
await library.start();
const adapterStart = p.onStart;
p.onStart = (app) => {
  adapterStart?.(app);
  app.store.library = library;
  library.attach(app.store);
  // The top bar's account menu says where collections are saved.
  library.addEventListener("change", () => app.requestUpdate());
  // The top bar's organization switcher: loaded on sign-in, cleared on sign-out.
  let orgAccount;
  const refreshOrgs = () => {
    const a = library.client.account();
    const key = a ? `${a.server} ${a.userId}` : "";
    if (key === orgAccount) return;
    orgAccount = key;
    import("./orgs.js").then(({ loadOrgs }) => loadOrgs(app.store, library.client));
  };
  library.addEventListener("change", refreshOrgs);
  // Team environments of the current team org (teamenvs.js).
  import("./teamenvs.js").then(({ TeamEnvs }) => {
    app.store.teamEnvs = new TeamEnvs(app.store, library.client);
    app.store.teamEnvs.start();
  });
  app.store.addEventListener("change", refreshOrgs);
  refreshOrgs();
  // Git sync of the library through the HITOP Cloud GitHub App (git.js): web build only, the
  // desktop has its own Git page for folders and no web origin to come back to from GitHub.
  if (p.name === "web") startGit(app);
};
async function startGit(app) {
  const { GitState } = await import("./git.js");
  const git = new GitState(library);
  app.store.git = git;
  git.addEventListener("change", () => {
    // Re-render the views (the store's version) without saving anything.
    app.store.version = (app.store.version ?? 0) + 1;
    app.store.dispatchEvent(new Event("change"));
  });
  git.start();
  // Back from GitHub while saving a collection to it: that collection's dialog again.
  const id = git.returned?.collection;
  const c = id != null && app.store.collections.find((x) => String(x.id) === id);
  if (c) {
    app.store.show("collection", c.id);
    app.openShare(c, "github");
  }
}
await import("./views/hitop-app.js");
