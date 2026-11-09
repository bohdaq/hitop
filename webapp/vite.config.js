import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

// The web app at hitop.rws8.tech/app/. The request logic is shared with the
// extension build: services come from ../frontend/src/services (plain JS in
// .jsx files, no React), so storage keys and shapes stay identical.
//
// `--mode extension` (npm run build:extension, HITOP Next A8): the browser extensions' pages.
// Relative paths, a second page (sandbox.html, the script runner) and no inline script: an
// extension page's CSP allows only its own files, so index.html's theme boot script becomes boot.js.
const inlineBoot = {
  name: "hitop-extension-boot",
  apply: "build",
  enforce: "post",
  generateBundle(_options, bundle) {
    const page = bundle["index.html"];
    if (!page) return;
    const html = String(page.source);
    const m = /<script>([\s\S]*?)<\/script>/.exec(html);
    if (!m) return;
    this.emitFile({ type: "asset", fileName: "boot.js", source: m[1] });
    page.source = html.replace(m[0], '<script src="./boot.js"></script>');
  },
};

// The Firefox extension's script runner (src/platform/wasm-scripts.js): hitop-scripts-wasm, built
// into src/wasm/pkg-scripts/ only by npm run build:extension; the other builds get a stub.
const scriptsWasm = (extension) => {
  const dir = fileURLToPath(new URL("./src/wasm/", import.meta.url));
  return extension
    ? [
        { find: /^hitop-scripts-wasm\/wasm/, replacement: `${dir}pkg-scripts/hitop_scripts_wasm_bg.wasm` },
        { find: /^hitop-scripts-wasm$/, replacement: `${dir}pkg-scripts/hitop_scripts_wasm.js` },
      ]
    : [
        { find: /^hitop-scripts-wasm\/wasm/, replacement: `${dir}scripts-stub.js` },
        { find: /^hitop-scripts-wasm$/, replacement: `${dir}scripts-stub.js` },
      ];
};

export default defineConfig(({ mode }) =>
  mode === "extension"
    ? {
        base: "./",
        plugins: [inlineBoot],
        resolve: { alias: scriptsWasm(true) },
        // The script runner is a module worker (Firefox 114+; the manifest asks for 115 ESR).
        worker: { format: "es" },
        build: {
          outDir: "dist-extension",
          emptyOutDir: true,
          rollupOptions: { input: { index: "index.html", sandbox: "sandbox.html" } },
        },
        server: { port: 3002, fs: { allow: [".."] } },
      }
    : {
        base: "/app/",
        resolve: { alias: scriptsWasm(false) },
        worker: { format: "es" },
        build: { outDir: "dist", emptyOutDir: true },
        server: { port: 3001, fs: { allow: [".."] } },
      },
);
