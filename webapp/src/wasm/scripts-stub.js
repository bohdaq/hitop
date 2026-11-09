// Stands in for hitop-scripts-wasm (src/wasm/pkg-scripts/, built only by npm run build:extension)
// in the web and desktop builds, which never load the Firefox script runner (vite.config.js alias).
export default async function init() {
  throw new Error("the script runner is in the extension build only");
}
export const runPreScript = () => {
  throw new Error("the script runner is in the extension build only");
};
export const runPostScript = runPreScript;
