// Team environments (sync-server teamenvs.rs): environments a team organization's members share
// through HITOP Cloud. In the store an environment with `team: <org id>` (and `org`) is one; the
// server gets it without secret values (the names stay in `secrets`), and each member keeps their
// own values in this browser (localStorage `hitop_team_secrets` {env id: {name: value}}).
// Loaded when a team org becomes the current one, again on focus and every 30 s; edits are pushed
// a second after a save (a stale copy takes the server's version and says so).

const SECRETS_KEY = "hitop_team_secrets";
const PUSH_DELAY = 1000;
const PULL_EVERY = 30_000;

const readSecrets = () => {
  try {
    return JSON.parse(localStorage.getItem(SECRETS_KEY) || "{}") || {};
  } catch {
    return {};
  }
};
const writeSecrets = (m) => {
  try {
    localStorage.setItem(SECRETS_KEY, JSON.stringify(m));
  } catch {
    // storage blocked: values last for this page
  }
};

function hash(value) {
  const text = JSON.stringify(value);
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return `${h.toString(16)}-${text.length}`;
}

/** What the server keeps of `env`: secret variables empty. */
export function teamData(env) {
  const names = env.secrets || [];
  const variables = Object.fromEntries(Object.entries(env.variables || {}).map(([k, v]) => [k, names.includes(k) ? "" : v]));
  return { name: env.name, variables, secrets: names };
}

export class TeamEnvs {
  constructor(store, client) {
    this.store = store;
    this.client = client;
    this.known = {}; // env id → {org, version, hash}
    this.loadedOrg = null;
  }

  start() {
    this.store.addEventListener("change", () => {
      const org = this.teamOrg();
      if (org && org !== this.loadedOrg) this.load(org);
      else if (org) this.schedulePush();
    });
    addEventListener("focus", () => this.teamOrg() && this.load(this.teamOrg()));
    setInterval(() => document.visibilityState === "visible" && this.teamOrg() && this.load(this.teamOrg()), PULL_EVERY);
  }

  /** The current org when it is a team org (signed in). */
  teamOrg() {
    const o = this.store.currentOrgInfo;
    return o && !o.personal && this.client.account() ? o.id : null;
  }

  async load(org) {
    if (this.loading) return this.loading;
    this.loading = (async () => {
      try {
        await this.push();
        const r = await this.client.api("GET", `/v1/orgs/${encodeURIComponent(org)}/environments`);
        const secrets = readSecrets();
        const fresh = r.environments.map((e) => {
          this.known[e.id] = { org, version: e.version, hash: hash(teamData(e.data)) };
          const local = this.store.environments.find((x) => x.id === e.id);
          const values = { ...(secrets[e.id] || {}) };
          // A value typed here before the last push is kept.
          for (const n of e.data.secrets || []) if (local?.variables?.[n]) values[n] = local.variables[n];
          return { ...e.data, id: e.id, org, team: org, updatedBy: e.updatedBy, variables: { ...e.data.variables, ...values } };
        });
        const ids = new Set(fresh.map((e) => e.id));
        for (const [id, k] of Object.entries(this.known)) if (k.org === org && !ids.has(id)) delete this.known[id];
        const s = this.store;
        const before = JSON.stringify(s.environments.filter((e) => e.team === org));
        s.environments = [...s.environments.filter((e) => e.team !== org), ...fresh];
        this.loadedOrg = org;
        if (JSON.stringify(fresh) !== before) s.emit();
      } catch {
        // offline or no access: keep what's here
        this.loadedOrg = org;
      } finally {
        this.loading = null;
      }
    })();
    return this.loading;
  }

  schedulePush() {
    clearTimeout(this.timer);
    this.timer = setTimeout(() => this.push(), PUSH_DELAY);
  }

  async push() {
    if (this.pushing) return this.schedulePush();
    const s = this.store;
    this.pushing = true;
    try {
      const secrets = readSecrets();
      const here = s.environments.filter((e) => e.team);
      for (const env of here) {
        const names = env.secrets || [];
        secrets[env.id] = Object.fromEntries(names.filter((n) => env.variables?.[n]).map((n) => [n, env.variables[n]]));
        const data = teamData(env);
        const h = hash(data);
        const k = this.known[env.id];
        if (k?.hash === h && k.org === env.team) continue;
        const r = await this.client.call("PUT", `/v1/orgs/${encodeURIComponent(env.team)}/environments/${encodeURIComponent(env.id)}`, { data, baseVersion: k?.org === env.team ? k.version : 0 }, { allow: [409] });
        if (r.status === 409) {
          const cur = r.json.current;
          if (cur) {
            this.known[env.id] = { org: env.team, version: cur.version, hash: hash(teamData(cur.data)) };
            s.environments = s.environments.map((e) => (e.id === env.id ? { ...cur.data, id: env.id, org: env.team, team: env.team, variables: { ...cur.data.variables, ...secrets[env.id] } } : e));
            s.notify(`${env.name} was changed by ${cur.updatedBy || "someone in the organization"}: showing theirs.`);
            s.emit();
          } else delete this.known[env.id];
        } else this.known[env.id] = { org: env.team, version: r.json.version, hash: h };
      }
      // Gone from here (deleted, or no longer shared): gone for the team.
      const ids = new Set(here.map((e) => e.id));
      for (const [id, k] of Object.entries(this.known)) {
        if (ids.has(id)) continue;
        await this.client.call("DELETE", `/v1/orgs/${encodeURIComponent(k.org)}/environments/${encodeURIComponent(id)}?baseVersion=${k.version}`, undefined, { allow: [409] }).catch(() => {});
        delete this.known[id];
        delete secrets[id];
      }
      writeSecrets(secrets);
    } catch {
      this.schedulePush();
    } finally {
      this.pushing = false;
    }
  }

  /** Share `envId` with the current team org, or stop sharing it (it stays here, this org only). */
  setShared(envId, shared) {
    const org = this.teamOrg();
    if (!org) return;
    const s = this.store;
    s.environments = s.environments.map((e) => {
      if (e.id !== envId) return e;
      const { team: _t, updatedBy: _u, ...rest } = e;
      return shared ? { ...rest, org, team: org } : { ...rest, org };
    });
    s.emit();
    this.push();
  }
}
