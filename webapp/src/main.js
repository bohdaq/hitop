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
// hitop sync (web and desktop): shared collections kept in app storage, and the Sync page.
const { withSyncFolders } = await import("./sync.js");
const { syncPage } = await import("./views/sync-view.js");
p.folders = withSyncFolders(p.folders);
p.pages = [...(p.pages ?? []), syncPage];
await import("./views/hitop-app.js");
