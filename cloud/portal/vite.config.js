import { execSync } from "node:child_process";
import { defineConfig } from "vite";

// The HITOP Cloud portal, served by hitop-server at cloud.hitop.rws8.tech (the front rws adds the
// /cloud prefix, so the files live in www/cloud/). Relative asset paths work at both
// cloud.hitop.rws8.tech/ and hitop.rws8.tech/cloud/.
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
  base: "./",
  define: { __HITOP_COMMIT__: JSON.stringify(commit()) },
  build: { outDir: "dist", emptyOutDir: true },
  server: { port: 3002, fs: { allow: ["../.."] } },
});
