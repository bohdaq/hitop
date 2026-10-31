import { defineConfig } from "vite";

// The web app at hitop.rws8.tech/app/. The request logic is shared with the
// extension build: services come from ../frontend/src/services (plain JS in
// .jsx files, no React), so storage keys and shapes stay identical.
export default defineConfig({
  base: "/app/",
  build: { outDir: "dist", emptyOutDir: true },
  server: { port: 3001, fs: { allow: [".."] } },
});
