# Change management policy

Status: draft for the owner's approval (C1, 2026-11-02). Review yearly.

Change management is the git history plus CI. No other tooling.

1. **Every change is a commit on `main`** of `github.com/bohdaq/hitop`, with a `Signed-off-by`
   line (DCO, checked by `.github/workflows/dco.yml`). Commits are small, one subject each.
2. **Tests come with the change.** A behaviour change carries tests; the tranche's "Done when"
   checks (spec/CLOUD_SERVICE.md, ROADMAP.md) are run before it is called done.
3. **CI must be green on `main`** (`.github/workflows/ci.yml`, `licenses.yml`, `fuzz.yml`):
   unit, integration and end-to-end tests on SQLite and Postgres, released clients against the new
   server (job `compat`), clippy, license and advisory checks. A red run is fixed before anything
   else ships.
4. **Database migrations are additive** and tested as an upgrade of a database shaped like the
   live one (`sync-server/tests/migration.rs`, `tests/postgres.rs`). The format of collections
   (spec/collection-format.md, v1) only grows.
5. **Deploys ship a commit that is green on CI**, from the scripts in the repository
   (`deploy/deploy.sh`, `sync-server/deploy/install.sh`, which back up the binary, the env file and
   the database first). Each deploy is recorded in `../ops-log.md`: date, what, the commit, who,
   the checks run after it.
6. **Configuration changes on the VM** (front rws routes, env files) follow `ROADMAP.md` section
   3: back the file up, `rws --check` before restarting, check every site still answers, roll back
   at once on a failure; recorded in the ops log.
7. **Emergency changes** follow the same steps; only the ordering may change (deploy, then the
   commit and CI within a day), and the ops log says why.
8. **Releases** of clients are tags (`cli-v*`, `desktop-v*`) built by GitHub Actions; the owner
   pre-approved them (ROADMAP section 2).
