import { execSync } from "node:child_process";
import { defineConfig } from "vite";

// The HITOP Cloud portal, served by hitop-server at cloud.hitop.rws8.tech (the front rws adds the
// /cloud prefix, so the files live in www/cloud/). Pages have real paths (/orgs/<id>/plan), so the
// assets are addressed from the root and hitop-server answers unknown paths under /cloud with
// index.html (RWS_CONFIG_SPA_FALLBACK_ROUTES, deploy/hitop-server.service). The old address
// hitop.rws8.tech/cloud/ is redirected to cloud.hitop.rws8.tech (deploy/rws-routes.toml).
//
// AGPL section 13: the footer names the commit this build is made from and links its source.
function commit() {
  if (process.env.HITOP_COMMIT) return process.env.HITOP_COMMIT;
  try {
    return execSync("git rev-parse --short=12 HEAD", { stdio: ["ignore", "pipe", "ignore"] }).toString().trim();
  } catch {
    return "main";
  }
}

export default defineConfig({
  base: "/",
  define: { __HITOP_COMMIT__: JSON.stringify(commit()) },
  build: { outDir: "dist", emptyOutDir: true },
  server: { port: 3002, fs: { allow: ["../.."] } },
});
