// One organization's data for its pages: the org, its members, usage and billing in one task,
// cached under ["org", id] so Members, Plan and Settings share a fetch and a mutation refreshes all.
import { TaskController, taskCache } from "@lit-material/task";
import { session } from "./session.js";

export const enc = (id) => encodeURIComponent(id);

export function orgTask(host, idOf) {
  return new TaskController(host, {
    task: async ([id], signal) => {
      const api = session.api;
      const base = `/v1/orgs/${enc(id)}`;
      const [org, members, usage, billing] = await Promise.all([
        api.get(base, { signal }),
        api.get(`${base}/members`, { signal }),
        api.get(`${base}/usage`, { signal }),
        // An older server has no billing: the plan page then shows the plan only.
        api.get(`${base}/billing`, { signal }).catch(() => null),
      ]);
      return { org, members, usage, billing };
    },
    args: () => [idOf()],
    key: ([id]) => ["org", id],
  });
}

/** After a change to an organization: its pages and the switcher fetch again. */
export function refreshOrg(id) {
  taskCache.invalidate(["org", id]);
  taskCache.invalidate(["orgs"]);
}

export const isAdmin = (org) => org.role === "owner" || org.role === "admin";
export const isOwner = (org) => org.role === "owner";
