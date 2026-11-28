// One way to call a plugin hook on any platform (docs/PLUGINS_THREAT_MODEL.md, part 3): the
// platform runs it (the browser's plugins worker, or the desktop's sandboxed child), then the answer
// is checked by hitop-core's validator through hitop-wasm, whatever the platform did already
// (decision 6). Viewer, importer and snippet screens and `${plugin.…}` values all come through here.
import { pluginUnavailable } from "../../shared/services/variableInterpolation.js";
import { platform } from "./platform/index.js";
import { loadFormat } from "./wasm/format.js";

/** The installed plugins that have `hook` (empty where there are no plugins or the list fails). */
export async function pluginsWith(hook) {
  const plugins = platform().plugins;
  if (!plugins) return [];
  try {
    return (await plugins.list()).filter((p) => p.hooks?.includes(hook));
  } catch {
    return [];
  }
}

/** The checked answer of `hook` of plugin `name`. `value` takes text arguments only (decision 11). */
export async function callHook(name, hook, args) {
  const plugins = platform().plugins;
  if (!plugins) throw new Error(pluginUnavailable(name));
  if (hook === "value" && !args.every((a) => typeof a === "string")) throw new Error(`plugin '${name}': a value call's arguments are text`);
  const answer = await plugins.call(name, hook, args);
  return (await loadFormat()).pluginAnswer(name, hook, answer);
}
