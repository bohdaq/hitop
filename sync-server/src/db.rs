//! The database: SQLite (default, `rusqlite` with SQLite bundled) or Postgres (feature
//! `postgres`). One connection behind a mutex, and every call runs in a transaction, so the
//! checks the API makes (version compare-and-swap, member lists) can't race. That also means one
//! server process per database; see the self-host guide.
//!
//! SQL is written once with `$1` placeholders and portable statements (`ON CONFLICT`, `BIGINT`);
//! SQLite gets `?1`. Parameters are never NULL (Postgres would need the NULL's type), so optional
//! values are stored as empty strings.

use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Int(i64),
    Text(String),
    Bytes(Vec<u8>),
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Int(v)
    }
}
impl From<u32> for Value {
    fn from(v: u32) -> Self {
        Value::Int(v.into())
    }
}
impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Int(v.into())
    }
}
impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::Text(v.to_string())
    }
}
impl From<&String> for Value {
    fn from(v: &String) -> Self {
        Value::Text(v.clone())
    }
}
impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::Text(v)
    }
}
impl From<&[u8]> for Value {
    fn from(v: &[u8]) -> Self {
        Value::Bytes(v.to_vec())
    }
}
impl From<Vec<u8>> for Value {
    fn from(v: Vec<u8>) -> Self {
        Value::Bytes(v)
    }
}

#[derive(Debug, Clone)]
pub struct Row(Vec<Value>);

impl Row {
    /// Every value of the row, in column order.
    pub fn values(&self) -> &[Value] {
        &self.0
    }
    pub fn int(&self, i: usize) -> i64 {
        match &self.0[i] {
            Value::Int(v) => *v,
            _ => 0,
        }
    }
    pub fn text(&self, i: usize) -> String {
        match &self.0[i] {
            Value::Text(v) => v.clone(),
            _ => String::new(),
        }
    }
    pub fn bytes(&self, i: usize) -> Vec<u8> {
        match &self.0[i] {
            Value::Bytes(v) => v.clone(),
            _ => Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct DbError(pub String);

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "database: {}", self.0)
    }
}

trait Backend: Send {
    fn execute(&mut self, sql: &str, params: &[Value]) -> Result<u64, DbError>;
    fn query(&mut self, sql: &str, params: &[Value]) -> Result<Vec<Row>, DbError>;
    fn batch(&mut self, sql: &str) -> Result<(), DbError>;
    fn begin(&self) -> &'static str;
    fn ddl(&self, sql: &str) -> String;
    /// `sqlite` or `postgres`.
    fn kind(&self) -> &'static str;
    /// The column names of `table`, in table order.
    fn columns(&mut self, table: &str) -> Result<Vec<String>, DbError>;
}

struct Sqlite(rusqlite::Connection);

fn sqlite_sql(sql: &str) -> String {
    sql.replace('$', "?")
}

fn sqlite_params(params: &[Value]) -> Vec<rusqlite::types::Value> {
    params
        .iter()
        .map(|v| match v {
            Value::Null => rusqlite::types::Value::Null,
            Value::Int(i) => rusqlite::types::Value::Integer(*i),
            Value::Text(s) => rusqlite::types::Value::Text(s.clone()),
            Value::Bytes(b) => rusqlite::types::Value::Blob(b.clone()),
        })
        .collect()
}

impl Backend for Sqlite {
    fn execute(&mut self, sql: &str, params: &[Value]) -> Result<u64, DbError> {
        let mut stmt = self.0.prepare_cached(&sqlite_sql(sql)).map_err(|e| DbError(e.to_string()))?;
        let n = stmt.execute(rusqlite::params_from_iter(sqlite_params(params))).map_err(|e| DbError(e.to_string()))?;
        Ok(n as u64)
    }

    fn query(&mut self, sql: &str, params: &[Value]) -> Result<Vec<Row>, DbError> {
        let mut stmt = self.0.prepare_cached(&sqlite_sql(sql)).map_err(|e| DbError(e.to_string()))?;
        let columns = stmt.column_count();
        let mut rows = stmt.query(rusqlite::params_from_iter(sqlite_params(params))).map_err(|e| DbError(e.to_string()))?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(|e| DbError(e.to_string()))? {
            let mut values = Vec::with_capacity(columns);
            for i in 0..columns {
                use rusqlite::types::ValueRef;
                values.push(match row.get_ref(i).map_err(|e| DbError(e.to_string()))? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(v) => Value::Int(v),
                    ValueRef::Real(v) => Value::Int(v as i64),
                    ValueRef::Text(t) => Value::Text(String::from_utf8_lossy(t).into_owned()),
                    ValueRef::Blob(b) => Value::Bytes(b.to_vec()),
                });
            }
            out.push(Row(values));
        }
        Ok(out)
    }

    fn batch(&mut self, sql: &str) -> Result<(), DbError> {
        self.0.execute_batch(sql).map_err(|e| DbError(e.to_string()))
    }

    fn begin(&self) -> &'static str {
        "BEGIN IMMEDIATE"
    }

    fn ddl(&self, sql: &str) -> String {
        sql.replace("{BLOB}", "BLOB")
    }

    fn kind(&self) -> &'static str {
        "sqlite"
    }

    fn columns(&mut self, table: &str) -> Result<Vec<String>, DbError> {
        Ok(self.query("SELECT name FROM pragma_table_info($1) ORDER BY cid", &[table.into()])?.iter().map(|r| r.text(0)).collect())
    }
}

#[cfg(feature = "postgres")]
struct Postgres(postgres::Client);

#[cfg(feature = "postgres")]
impl Postgres {
    fn params(params: &[Value]) -> Result<Vec<Box<dyn postgres::types::ToSql + Sync>>, DbError> {
        params
            .iter()
            .map(|v| -> Result<Box<dyn postgres::types::ToSql + Sync>, DbError> {
                Ok(match v {
                    Value::Null => return Err(DbError("NULL parameters aren't supported".into())),
                    Value::Int(i) => Box::new(*i),
                    Value::Text(s) => Box::new(s.clone()),
                    Value::Bytes(b) => Box::new(b.clone()),
                })
            })
            .collect()
    }
}

#[cfg(feature = "postgres")]
impl Backend for Postgres {
    fn execute(&mut self, sql: &str, params: &[Value]) -> Result<u64, DbError> {
        let owned = Self::params(params)?;
        let refs: Vec<&(dyn postgres::types::ToSql + Sync)> = owned.iter().map(|b| b.as_ref()).collect();
        self.0.execute(sql, &refs).map_err(|e| DbError(e.to_string()))
    }

    fn query(&mut self, sql: &str, params: &[Value]) -> Result<Vec<Row>, DbError> {
        use postgres::types::Type;
        let owned = Self::params(params)?;
        let refs: Vec<&(dyn postgres::types::ToSql + Sync)> = owned.iter().map(|b| b.as_ref()).collect();
        let rows = self.0.query(sql, &refs).map_err(|e| DbError(e.to_string()))?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let mut values = Vec::with_capacity(row.len());
            for (i, column) in row.columns().iter().enumerate() {
                let err = |e: postgres::Error| DbError(e.to_string());
                let t = column.type_();
                values.push(if *t == Type::INT8 {
                    row.try_get::<_, Option<i64>>(i).map_err(err)?.map_or(Value::Null, Value::Int)
                } else if *t == Type::INT4 {
                    row.try_get::<_, Option<i32>>(i).map_err(err)?.map_or(Value::Null, |v| Value::Int(v.into()))
                } else if *t == Type::BYTEA {
                    row.try_get::<_, Option<Vec<u8>>>(i).map_err(err)?.map_or(Value::Null, Value::Bytes)
                } else {
                    row.try_get::<_, Option<String>>(i).map_err(err)?.map_or(Value::Null, Value::Text)
                });
            }
            out.push(Row(values));
        }
        Ok(out)
    }

    fn batch(&mut self, sql: &str) -> Result<(), DbError> {
        self.0.batch_execute(sql).map_err(|e| DbError(e.to_string()))
    }

    fn begin(&self) -> &'static str {
        "BEGIN"
    }

    fn ddl(&self, sql: &str) -> String {
        sql.replace("{BLOB}", "BYTEA")
    }

    fn kind(&self) -> &'static str {
        "postgres"
    }

    fn columns(&mut self, table: &str) -> Result<Vec<String>, DbError> {
        // The table the connection's search_path finds (tests use a schema per test).
        let sql = "SELECT column_name::text FROM information_schema.columns WHERE table_name = $1 AND table_schema = (SELECT n.nspname::text FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE c.oid = to_regclass($1)) ORDER BY ordinal_position";
        Ok(self.query(sql, &[table.into()])?.iter().map(|r| r.text(0)).collect())
    }
}

pub struct Db {
    backend: Mutex<Box<dyn Backend>>,
}

/// A transaction: every query of one API call.
pub struct Tx<'a> {
    backend: &'a mut dyn Backend,
}

impl Tx<'_> {
    pub fn execute(&mut self, sql: &str, params: &[Value]) -> Result<u64, DbError> {
        self.backend.execute(sql, params)
    }

    pub fn query(&mut self, sql: &str, params: &[Value]) -> Result<Vec<Row>, DbError> {
        self.backend.query(sql, params)
    }

    pub fn query_one(&mut self, sql: &str, params: &[Value]) -> Result<Option<Row>, DbError> {
        Ok(self.backend.query(sql, params)?.into_iter().next())
    }

    /// `sqlite` or `postgres`.
    pub fn kind(&self) -> &'static str {
        self.backend.kind()
    }

    /// The column names of `table` (empty when there is no such table).
    pub fn columns(&mut self, table: &str) -> Result<Vec<String>, DbError> {
        self.backend.columns(table)
    }
}

/// Schema migrations, in order; `{BLOB}` becomes BLOB or BYTEA. Never edit a released entry: add
/// a new one. 1: T23 (accounts, workspaces, blobs). 2: T24 teams (invites, runner accounts).
/// 3: T25 SSO identities and the audit log (`audit_log` is append-only: rows are only ever
/// inserted, and deleted by the retention purge, audit.rs). 4: HITOP Cloud C1 organizations
/// (orgs.rs), entitlements and usage (limits.rs), API tokens (tokens.rs): every person gets a
/// personal org (`personal-<user id>`, Free) that owns the workspaces they own; after the SQL,
/// `limits::grandfather` raises each personal org's limits to its usage (`migrate`), so no
/// existing account is over a limit. 5: HITOP Cloud C2 key bundles (recovery.rs), one opaque
/// blob per account and unlock method (`phrase` now; a password later). 6: HITOP Cloud C1b
/// passwords (password.rs). 7: HITOP Cloud C3 billing (billing.rs): the provider ids (Stripe since 2026-11-03; the columns
/// keep their `ls_` names) and
/// ordering mark on `entitlements`, `billing_events` (ids and outcomes of webhook deliveries,
/// never their bodies) and `trials`.
/// 8: HITOP Cloud C4 run history (runs.rs), alerts (alerts.rs), shared environment values
/// (shares.rs). 9: HITOP Cloud C5 SSO per org, domains, audit anchors, rotation requests.
/// 10: HITOP Cloud C6 SCIM provisioning (scim.rs): per-org tokens (hashes), the directory's users
/// and groups as the IdP sent them, a group's org role and workspace roles, and the workspace
/// grants SCIM made (so it only ever takes back what it gave). 11: device keys an org admin took
/// out of an org workspace (admin.rs `remove_device`): no wrap of that workspace is ever stored or
/// asked for them again (the key itself and the person's other workspaces stay).
const MIGRATIONS: &[&str] = &[r"
CREATE TABLE users (
  id TEXT PRIMARY KEY,
  email TEXT NOT NULL UNIQUE,
  created_at BIGINT NOT NULL
);
CREATE TABLE sessions (
  token_hash TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  created_at BIGINT NOT NULL,
  expires_at BIGINT NOT NULL
);
CREATE INDEX sessions_user ON sessions(user_id);
CREATE TABLE magic_links (
  token_hash TEXT PRIMARY KEY,
  email TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  expires_at BIGINT NOT NULL,
  used BIGINT NOT NULL DEFAULT 0
);
CREATE INDEX magic_links_email ON magic_links(email, created_at);
CREATE TABLE passkeys (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  data TEXT NOT NULL,
  created_at BIGINT NOT NULL
);
CREATE INDEX passkeys_user ON passkeys(user_id);
CREATE TABLE device_keys (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  public_key {BLOB} NOT NULL,
  created_at BIGINT NOT NULL
);
CREATE INDEX device_keys_user ON device_keys(user_id);
CREATE TABLE workspaces (
  id TEXT PRIMARY KEY,
  epoch BIGINT NOT NULL,
  seq BIGINT NOT NULL,
  bytes BIGINT NOT NULL,
  created_at BIGINT NOT NULL
);
CREATE TABLE members (
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  role TEXT NOT NULL,
  PRIMARY KEY (workspace_id, user_id)
);
CREATE INDEX members_user ON members(user_id);
CREATE TABLE wraps (
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  epoch BIGINT NOT NULL,
  key_id TEXT NOT NULL REFERENCES device_keys(id) ON DELETE CASCADE,
  wrapped {BLOB} NOT NULL,
  PRIMARY KEY (workspace_id, epoch, key_id)
);
CREATE TABLE blobs (
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  id TEXT NOT NULL,
  version BIGINT NOT NULL,
  vv TEXT NOT NULL,
  epoch BIGINT NOT NULL,
  deleted BIGINT NOT NULL,
  content {BLOB} NOT NULL,
  seq BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  PRIMARY KEY (workspace_id, id)
);
CREATE INDEX blobs_seq ON blobs(workspace_id, seq);
", r"
ALTER TABLE users ADD COLUMN runner_of TEXT NOT NULL DEFAULT '';
CREATE TABLE invites (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  token_hash TEXT NOT NULL UNIQUE,
  email TEXT NOT NULL,
  role TEXT NOT NULL,
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  expires_at BIGINT NOT NULL,
  accepted_by TEXT NOT NULL DEFAULT '',
  accepted_at BIGINT NOT NULL DEFAULT 0
);
CREATE INDEX invites_workspace ON invites(workspace_id);
", r"
CREATE TABLE oidc_identities (
  issuer TEXT NOT NULL,
  subject TEXT NOT NULL,
  user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  email TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  last_used_at BIGINT NOT NULL,
  PRIMARY KEY (issuer, subject)
);
CREATE INDEX oidc_identities_user ON oidc_identities(user_id);
CREATE TABLE audit_log (
  workspace_id TEXT NOT NULL,
  seq BIGINT NOT NULL,
  at BIGINT NOT NULL,
  event TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  actor_email TEXT NOT NULL,
  target TEXT NOT NULL,
  detail TEXT NOT NULL,
  ip TEXT NOT NULL,
  user_agent TEXT NOT NULL,
  prev_hash TEXT NOT NULL,
  hash TEXT NOT NULL,
  PRIMARY KEY (workspace_id, seq)
);
CREATE INDEX audit_log_at ON audit_log(at);
CREATE INDEX audit_log_actor ON audit_log(workspace_id, actor_id, seq);
CREATE TABLE audit_chain (
  workspace_id TEXT PRIMARY KEY,
  head_seq BIGINT NOT NULL,
  head_hash TEXT NOT NULL,
  anchor_seq BIGINT NOT NULL,
  anchor_hash TEXT NOT NULL
);
", r"
CREATE TABLE orgs (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  personal_of TEXT NOT NULL DEFAULT '',
  plan TEXT NOT NULL DEFAULT 'free',
  region TEXT NOT NULL DEFAULT 'eu',
  billing_ref TEXT NOT NULL DEFAULT '',
  created_by TEXT NOT NULL DEFAULT '',
  created_at BIGINT NOT NULL
);
CREATE INDEX orgs_personal ON orgs(personal_of);
CREATE TABLE org_members (
  org_id TEXT NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
  user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  role TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  PRIMARY KEY (org_id, user_id)
);
CREATE INDEX org_members_user ON org_members(user_id);
CREATE TABLE org_invites (
  org_id TEXT NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
  email TEXT NOT NULL,
  role TEXT NOT NULL,
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  PRIMARY KEY (org_id, email)
);
CREATE INDEX org_invites_email ON org_invites(email);
ALTER TABLE workspaces ADD COLUMN org_id TEXT NOT NULL DEFAULT '';
CREATE INDEX workspaces_org ON workspaces(org_id);
CREATE TABLE entitlements (
  org_id TEXT PRIMARY KEY REFERENCES orgs(id) ON DELETE CASCADE,
  plan TEXT NOT NULL,
  seats BIGINT NOT NULL DEFAULT 0,
  limits TEXT NOT NULL DEFAULT '{}',
  status TEXT NOT NULL DEFAULT 'active',
  period_end BIGINT NOT NULL DEFAULT 0,
  ls_customer_id TEXT NOT NULL DEFAULT '',
  ls_subscription_id TEXT NOT NULL DEFAULT '',
  ls_variant_id TEXT NOT NULL DEFAULT '',
  updated_at BIGINT NOT NULL
);
CREATE TABLE usage_counters (
  org_id TEXT NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
  metric TEXT NOT NULL,
  day BIGINT NOT NULL,
  count BIGINT NOT NULL,
  PRIMARY KEY (org_id, metric, day)
);
CREATE TABLE api_tokens (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  token_hash TEXT NOT NULL UNIQUE,
  scopes TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  expires_at BIGINT NOT NULL,
  last_used_at BIGINT NOT NULL DEFAULT 0
);
CREATE INDEX api_tokens_user ON api_tokens(user_id);
INSERT INTO orgs (id, name, personal_of, plan, region, billing_ref, created_by, created_at)
  SELECT 'personal-' || id, 'Personal', id, 'free', 'eu', '', id, created_at FROM users WHERE runner_of = '';
INSERT INTO org_members (org_id, user_id, role, created_at)
  SELECT 'personal-' || id, id, 'owner', created_at FROM users WHERE runner_of = '';
INSERT INTO entitlements (org_id, plan, seats, limits, status, period_end, updated_at)
  SELECT id, 'free', 0, '{}', 'active', 0, created_at FROM orgs;
UPDATE workspaces SET org_id = COALESCE((SELECT 'personal-' || MIN(m.user_id) FROM members m JOIN users u ON u.id = m.user_id
  WHERE m.workspace_id = workspaces.id AND m.role = 'owner' AND u.runner_of = ''), '');
", r"
CREATE TABLE recovery_blobs (
  user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  method TEXT NOT NULL,
  blob {BLOB} NOT NULL,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  PRIMARY KEY (user_id, method)
);
", r"
CREATE TABLE password_credentials (
  user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
  kdf TEXT NOT NULL,
  hash TEXT NOT NULL,
  peppered BIGINT NOT NULL DEFAULT 0,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL
);
CREATE TABLE password_tokens (
  token_hash TEXT PRIMARY KEY,
  email TEXT NOT NULL,
  purpose TEXT NOT NULL,
  kdf TEXT NOT NULL DEFAULT '',
  hash TEXT NOT NULL DEFAULT '',
  peppered BIGINT NOT NULL DEFAULT 0,
  created_at BIGINT NOT NULL,
  expires_at BIGINT NOT NULL,
  used BIGINT NOT NULL DEFAULT 0
);
CREATE INDEX password_tokens_email ON password_tokens(email, created_at);
CREATE TABLE server_secrets (
  name TEXT PRIMARY KEY,
  value {BLOB} NOT NULL
);
", r"
ALTER TABLE entitlements ADD COLUMN ls_item_id TEXT NOT NULL DEFAULT '';
ALTER TABLE entitlements ADD COLUMN ls_order_id TEXT NOT NULL DEFAULT '';
ALTER TABLE entitlements ADD COLUMN ls_updated_at BIGINT NOT NULL DEFAULT 0;
CREATE INDEX entitlements_ls_subscription ON entitlements(ls_subscription_id);
CREATE TABLE billing_events (
  id TEXT PRIMARY KEY,
  event_name TEXT NOT NULL,
  org_id TEXT NOT NULL DEFAULT '',
  subscription_id TEXT NOT NULL DEFAULT '',
  resource_updated_at BIGINT NOT NULL DEFAULT 0,
  test_mode BIGINT NOT NULL DEFAULT 0,
  outcome TEXT NOT NULL,
  received_at BIGINT NOT NULL
);
CREATE INDEX billing_events_org ON billing_events(org_id, received_at);
CREATE TABLE trials (
  org_id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL,
  email_domain TEXT NOT NULL,
  started_at BIGINT NOT NULL,
  ends_at BIGINT NOT NULL
);
CREATE INDEX trials_user ON trials(user_id);
CREATE INDEX trials_domain ON trials(email_domain);
", r"
CREATE TABLE runs (
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  id TEXT NOT NULL,
  uploader TEXT NOT NULL,
  collection TEXT NOT NULL DEFAULT '',
  environment TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL,
  started_at BIGINT NOT NULL,
  duration_ms BIGINT NOT NULL DEFAULT 0,
  requests BIGINT NOT NULL DEFAULT 0,
  failed BIGINT NOT NULL DEFAULT 0,
  failed_names TEXT NOT NULL DEFAULT '',
  detail {BLOB} NOT NULL,
  detail_bytes BIGINT NOT NULL DEFAULT 0,
  created_at BIGINT NOT NULL,
  PRIMARY KEY (workspace_id, id)
);
CREATE INDEX runs_series ON runs(workspace_id, collection, environment, created_at);
CREATE INDEX runs_created ON runs(created_at);
CREATE TABLE alert_settings (
  workspace_id TEXT PRIMARY KEY REFERENCES workspaces(id) ON DELETE CASCADE,
  settings TEXT NOT NULL,
  updated_by TEXT NOT NULL,
  updated_at BIGINT NOT NULL
);
CREATE TABLE alert_state (
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  collection TEXT NOT NULL,
  environment TEXT NOT NULL,
  last_status TEXT NOT NULL,
  last_run_at BIGINT NOT NULL,
  missed_alerted_at BIGINT NOT NULL DEFAULT 0,
  PRIMARY KEY (workspace_id, collection, environment)
);
CREATE TABLE alert_deliveries (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  event TEXT NOT NULL,
  channel TEXT NOT NULL,
  collection TEXT NOT NULL DEFAULT '',
  environment TEXT NOT NULL DEFAULT '',
  run_id TEXT NOT NULL DEFAULT '',
  failed_names TEXT NOT NULL DEFAULT '',
  outcome TEXT NOT NULL,
  attempts BIGINT NOT NULL DEFAULT 0,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL
);
CREATE INDEX alert_deliveries_ws ON alert_deliveries(workspace_id, created_at);
CREATE INDEX alert_deliveries_outcome ON alert_deliveries(outcome);
CREATE TABLE env_shares (
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  id TEXT NOT NULL,
  version BIGINT NOT NULL,
  sealed {BLOB} NOT NULL,
  created_by TEXT NOT NULL,
  updated_by TEXT NOT NULL,
  updated_at BIGINT NOT NULL,
  PRIMARY KEY (workspace_id, id)
);
CREATE TABLE env_share_wraps (
  workspace_id TEXT NOT NULL,
  share_id TEXT NOT NULL,
  key_id TEXT NOT NULL,
  user_id TEXT NOT NULL,
  version BIGINT NOT NULL,
  wrapped {BLOB} NOT NULL,
  PRIMARY KEY (workspace_id, share_id, key_id),
  FOREIGN KEY (workspace_id, share_id) REFERENCES env_shares(workspace_id, id) ON DELETE CASCADE
);
CREATE INDEX env_share_wraps_user ON env_share_wraps(workspace_id, user_id);
", r"
CREATE TABLE org_sso (
  org_id TEXT PRIMARY KEY REFERENCES orgs(id) ON DELETE CASCADE,
  issuer TEXT NOT NULL,
  client_id TEXT NOT NULL,
  client_secret {BLOB} NOT NULL,
  scopes TEXT NOT NULL,
  name TEXT NOT NULL,
  groups_claim TEXT NOT NULL,
  required_groups TEXT NOT NULL DEFAULT '',
  required BIGINT NOT NULL DEFAULT 0,
  assume_verified BIGINT NOT NULL DEFAULT 0,
  enabled BIGINT NOT NULL DEFAULT 1,
  updated_by TEXT NOT NULL,
  updated_at BIGINT NOT NULL
);
CREATE TABLE org_domains (
  org_id TEXT NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
  domain TEXT NOT NULL,
  token TEXT NOT NULL,
  verified_at BIGINT NOT NULL DEFAULT 0,
  checked_at BIGINT NOT NULL DEFAULT 0,
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  PRIMARY KEY (org_id, domain)
);
CREATE INDEX org_domains_domain ON org_domains(domain, verified_at);
CREATE TABLE org_audit_settings (
  org_id TEXT PRIMARY KEY REFERENCES orgs(id) ON DELETE CASCADE,
  mail_anchors BIGINT NOT NULL DEFAULT 1,
  public_anchors BIGINT NOT NULL DEFAULT 0,
  updated_by TEXT NOT NULL,
  updated_at BIGINT NOT NULL
);
CREATE TABLE audit_anchors (
  org_id TEXT NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
  day TEXT NOT NULL,
  body TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  mailed_at BIGINT NOT NULL DEFAULT 0,
  PRIMARY KEY (org_id, day)
);
ALTER TABLE workspaces ADD COLUMN rotation_requested_at BIGINT NOT NULL DEFAULT 0;
ALTER TABLE workspaces ADD COLUMN rotation_requested_by TEXT NOT NULL DEFAULT '';
", r"
CREATE TABLE scim_tokens (
  id TEXT PRIMARY KEY,
  org_id TEXT NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  token_hash TEXT NOT NULL UNIQUE,
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  last_used_at BIGINT NOT NULL DEFAULT 0
);
CREATE INDEX scim_tokens_org ON scim_tokens(org_id);
CREATE TABLE scim_users (
  id TEXT PRIMARY KEY,
  org_id TEXT NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
  user_name TEXT NOT NULL,
  external_id TEXT NOT NULL DEFAULT '',
  display_name TEXT NOT NULL DEFAULT '',
  name_json TEXT NOT NULL DEFAULT '{}',
  active BIGINT NOT NULL DEFAULT 1,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  UNIQUE (org_id, user_name)
);
CREATE TABLE scim_groups (
  id TEXT PRIMARY KEY,
  org_id TEXT NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
  display_name TEXT NOT NULL,
  external_id TEXT NOT NULL DEFAULT '',
  org_role TEXT NOT NULL DEFAULT '',
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL
);
CREATE INDEX scim_groups_org ON scim_groups(org_id);
CREATE TABLE scim_group_members (
  group_id TEXT NOT NULL REFERENCES scim_groups(id) ON DELETE CASCADE,
  user_id TEXT NOT NULL REFERENCES scim_users(id) ON DELETE CASCADE,
  PRIMARY KEY (group_id, user_id)
);
CREATE INDEX scim_group_members_user ON scim_group_members(user_id);
CREATE TABLE scim_group_workspaces (
  group_id TEXT NOT NULL REFERENCES scim_groups(id) ON DELETE CASCADE,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  role TEXT NOT NULL,
  PRIMARY KEY (group_id, workspace_id)
);
CREATE TABLE scim_grants (
  org_id TEXT NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  email TEXT NOT NULL,
  role TEXT NOT NULL,
  kind TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  PRIMARY KEY (workspace_id, email)
);
CREATE INDEX scim_grants_org ON scim_grants(org_id, email);
", r"
CREATE TABLE workspace_excluded_keys (
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  key_id TEXT NOT NULL REFERENCES device_keys(id) ON DELETE CASCADE,
  excluded_at BIGINT NOT NULL,
  excluded_by TEXT NOT NULL,
  PRIMARY KEY (workspace_id, key_id)
);
CREATE INDEX workspace_excluded_keys_key ON workspace_excluded_keys(key_id);
", r"
CREATE TABLE library_items (
  user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  org_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  item_id TEXT NOT NULL,
  version BIGINT NOT NULL,
  seq BIGINT NOT NULL,
  data TEXT NOT NULL,
  secrets {BLOB} NOT NULL,
  bytes BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  deleted BIGINT NOT NULL,
  PRIMARY KEY (user_id, kind, item_id)
);
CREATE INDEX library_items_seq ON library_items(user_id, seq);
CREATE INDEX library_items_org ON library_items(org_id);
"];

/// Steps in Rust that run in the same transaction as a migration's SQL (index = migration
/// number - 1).
fn after_migration(index: usize, tx: &mut Tx) -> Result<(), DbError> {
    if index == 3 {
        crate::limits::grandfather(tx)?;
    }
    Ok(())
}

impl Db {
    /// `sqlite:<path>`, `sqlite::memory:`, or `postgres://...` (feature `postgres`).
    pub fn open(url: &str) -> Result<Db, DbError> {
        Self::open_at(url, MIGRATIONS.len())
    }

    /// Opens and migrates only up to schema `version` (tests build older databases with it).
    #[doc(hidden)]
    pub fn open_at(url: &str, version: usize) -> Result<Db, DbError> {
        let backend: Box<dyn Backend> = if let Some(path) = url.strip_prefix("sqlite:") {
            let conn = if path == ":memory:" { rusqlite::Connection::open_in_memory() } else { rusqlite::Connection::open(path) }
                .map_err(|e| DbError(format!("{path}: {e}")))?;
            conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;")
                .map_err(|e| DbError(e.to_string()))?;
            Box::new(Sqlite(conn))
        } else if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            #[cfg(feature = "postgres")]
            {
                let client = postgres::Client::connect(url, postgres::NoTls).map_err(|e| DbError(e.to_string()))?;
                Box::new(Postgres(client))
            }
            #[cfg(not(feature = "postgres"))]
            return Err(DbError("this build has no Postgres support (build with --features postgres)".into()));
        } else {
            return Err(DbError(format!("unknown database URL {url:?} (use sqlite:<path> or postgres://...)")));
        };
        let db = Db { backend: Mutex::new(backend) };
        db.migrate(version)?;
        Ok(db)
    }

    /// The schema version this build migrates to.
    pub fn schema_version() -> usize {
        MIGRATIONS.len()
    }

    /// Every table the migrations create, in creation order (a table is created after the tables
    /// it references, so copying in this order satisfies every foreign key). Not `schema_version`.
    pub fn tables() -> Vec<String> {
        MIGRATIONS
            .iter()
            .flat_map(|sql| sql.lines())
            .filter_map(|line| line.trim().strip_prefix("CREATE TABLE "))
            .filter_map(|rest| rest.split([' ', '(']).next())
            .map(String::from)
            .collect()
    }

    fn migrate(&self, upto: usize) -> Result<(), DbError> {
        let mut guard = self.backend.lock().unwrap_or_else(|e| e.into_inner());
        let b = guard.as_mut();
        b.batch("CREATE TABLE IF NOT EXISTS schema_version (version BIGINT NOT NULL)")?;
        let current = b.query("SELECT COALESCE(MAX(version), 0) FROM schema_version", &[])?.first().map_or(0, |r| r.int(0));
        for (i, sql) in MIGRATIONS.iter().enumerate().take(upto).skip(current as usize) {
            let begin = b.begin();
            b.batch(begin)?;
            let ddl = b.ddl(sql);
            let applied = b
                .batch(&ddl)
                .and_then(|_| after_migration(i, &mut Tx { backend: &mut *b }))
                .and_then(|_| b.execute("INSERT INTO schema_version (version) VALUES ($1)", &[Value::Int(i as i64 + 1)]));
            match applied {
                Ok(_) => b.batch("COMMIT")?,
                Err(e) => {
                    let _ = b.batch("ROLLBACK");
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    /// One statement outside any transaction (`VACUUM INTO` can't run in one).
    pub fn execute_alone(&self, sql: &str, params: &[Value]) -> Result<u64, DbError> {
        let mut guard = self.backend.lock().unwrap_or_else(|e| e.into_inner());
        guard.as_mut().execute(sql, params)
    }

    /// `sqlite` or `postgres`.
    pub fn kind(&self) -> &'static str {
        self.backend.lock().unwrap_or_else(|e| e.into_inner()).kind()
    }

    /// Runs `f` in one transaction: committed on `Ok`, rolled back on `Err`.
    pub fn tx<T, E: From<DbError>>(&self, f: impl FnOnce(&mut Tx) -> Result<T, E>) -> Result<T, E> {
        self.run_tx(false, f)
    }

    /// Runs `f` in one read-only transaction that sees a single snapshot of the database
    /// (Postgres: repeatable read; SQLite: a deferred transaction, one snapshot in WAL mode), so
    /// stats and copies are consistent while the server keeps writing.
    pub fn read_tx<T, E: From<DbError>>(&self, f: impl FnOnce(&mut Tx) -> Result<T, E>) -> Result<T, E> {
        self.run_tx(true, f)
    }

    fn run_tx<T, E: From<DbError>>(&self, read: bool, f: impl FnOnce(&mut Tx) -> Result<T, E>) -> Result<T, E> {
        let mut guard = self.backend.lock().unwrap_or_else(|e| e.into_inner());
        let backend = guard.as_mut();
        let begin = match (read, backend.kind()) {
            (true, "postgres") => "BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY",
            (true, _) => "BEGIN",
            _ => backend.begin(),
        };
        backend.batch(begin)?;
        let mut tx = Tx { backend };
        match f(&mut tx) {
            Ok(v) => {
                tx.backend.batch("COMMIT")?;
                Ok(v)
            }
            Err(e) => {
                let _ = tx.backend.batch("ROLLBACK");
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_t23_database_migrates_to_the_current_schema() {
        // What the VM has: migration 1 only, with an account in it.
        let dir = std::env::temp_dir().join(format!("hitop-sync-db-v1-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.db");
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch("CREATE TABLE schema_version (version BIGINT NOT NULL); INSERT INTO schema_version (version) VALUES (1);").unwrap();
            conn.execute_batch(&MIGRATIONS[0].replace("{BLOB}", "BLOB")).unwrap();
            conn.execute("INSERT INTO users (id, email, created_at) VALUES ('u1', 'a@example.com', 1)", []).unwrap();
        }
        let db = Db::open(&format!("sqlite:{}", path.display())).unwrap();
        let rows = db.tx(|tx| tx.query("SELECT id, runner_of FROM users", &[])).unwrap();
        assert_eq!((rows[0].text(0), rows[0].text(1)), ("u1".to_string(), String::new()), "existing accounts are people");
        db.tx(|tx| tx.query("SELECT id, workspace_id, token_hash, accepted_by FROM invites", &[])).unwrap();
        let v = db.tx(|tx| tx.query("SELECT MAX(version) FROM schema_version", &[])).unwrap();
        assert_eq!(v[0].int(0), MIGRATIONS.len() as i64);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_t24_database_migrates_to_the_current_schema() {
        // What the VM has after T24: migrations 1 and 2, with a team in it.
        let dir = std::env::temp_dir().join(format!("hitop-sync-db-v2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.db");
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch("CREATE TABLE schema_version (version BIGINT NOT NULL); INSERT INTO schema_version (version) VALUES (1); INSERT INTO schema_version (version) VALUES (2);").unwrap();
            conn.execute_batch(&MIGRATIONS[0].replace("{BLOB}", "BLOB")).unwrap();
            conn.execute_batch(&MIGRATIONS[1].replace("{BLOB}", "BLOB")).unwrap();
            conn.execute_batch(
                "INSERT INTO users (id, email, created_at) VALUES ('u1', 'a@example.com', 1);
                 INSERT INTO workspaces (id, epoch, seq, bytes, created_at) VALUES ('w1', 1, 0, 0, 1);
                 INSERT INTO members (workspace_id, user_id, role) VALUES ('w1', 'u1', 'owner');
                 INSERT INTO invites (id, workspace_id, token_hash, email, role, created_by, created_at, expires_at) VALUES ('i1', 'w1', 'h', '', 'editor', 'u1', 1, 2);",
            )
            .unwrap();
        }
        let db = Db::open(&format!("sqlite:{}", path.display())).unwrap();
        let v = db.tx(|tx| tx.query("SELECT MAX(version) FROM schema_version", &[])).unwrap();
        assert_eq!(v[0].int(0), MIGRATIONS.len() as i64);
        let kept = db.tx(|tx| tx.query("SELECT m.role, i.role FROM members m JOIN invites i ON i.workspace_id = m.workspace_id", &[])).unwrap();
        assert_eq!((kept[0].text(0), kept[0].text(1)), ("owner".to_string(), "editor".to_string()), "existing teams are untouched");
        db.tx(|tx| tx.query("SELECT issuer, subject, user_id, email, created_at, last_used_at FROM oidc_identities", &[])).unwrap();
        db.tx(|tx| tx.query("SELECT workspace_id, seq, at, event, actor_id, actor_email, target, detail, ip, user_agent, prev_hash, hash FROM audit_log", &[])).unwrap();
        db.tx(|tx| tx.query("SELECT workspace_id, head_seq, head_hash, anchor_seq, anchor_hash FROM audit_chain", &[])).unwrap();
        // Identities go with their account.
        db.tx(|tx| tx.execute("INSERT INTO oidc_identities (issuer, subject, user_id, email, created_at, last_used_at) VALUES ('https://id', 's', 'u1', 'a@example.com', 1, 1)", &[])).unwrap();
        db.tx(|tx| tx.execute("DELETE FROM users WHERE id = 'u1'", &[])).unwrap();
        assert!(db.tx(|tx| tx.query("SELECT subject FROM oidc_identities", &[])).unwrap().is_empty());
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn migrates_once_and_rolls_back_on_error() {
        let dir = std::env::temp_dir().join(format!("hitop-sync-db-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let url = format!("sqlite:{}", dir.join("t.db").display());
        let db = Db::open(&url).unwrap();
        db.tx(|tx| tx.execute("INSERT INTO users (id, email, created_at) VALUES ($1, $2, $3)", &["u1".into(), "a@example.com".into(), 1i64.into()])).unwrap();
        let failed: Result<(), DbError> = db.tx(|tx| {
            tx.execute("INSERT INTO users (id, email, created_at) VALUES ($1, $2, $3)", &["u2".into(), "b@example.com".into(), 1i64.into()])?;
            Err(DbError("abort".into()))
        });
        assert!(failed.is_err());
        drop(db);
        // Reopening runs no migration twice and keeps committed rows only.
        let db = Db::open(&url).unwrap();
        let rows = db.tx(|tx| tx.query("SELECT id FROM users ORDER BY id", &[])).unwrap();
        assert_eq!(rows.iter().map(|r| r.text(0)).collect::<Vec<_>>(), ["u1"]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
