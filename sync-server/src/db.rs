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
}

/// Schema migrations, in order; `{BLOB}` becomes BLOB or BYTEA. Never edit a released entry: add
/// a new one. 1: T23 (accounts, workspaces, blobs). 2: T24 teams (invites, runner accounts).
/// 3: T25 SSO identities and the audit log (`audit_log` is append-only: rows are only ever
/// inserted, and deleted by the retention purge, audit.rs).
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
"];

impl Db {
    /// `sqlite:<path>`, `sqlite::memory:`, or `postgres://...` (feature `postgres`).
    pub fn open(url: &str) -> Result<Db, DbError> {
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
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<(), DbError> {
        let mut guard = self.backend.lock().unwrap_or_else(|e| e.into_inner());
        let b = guard.as_mut();
        b.batch("CREATE TABLE IF NOT EXISTS schema_version (version BIGINT NOT NULL)")?;
        let current = b.query("SELECT COALESCE(MAX(version), 0) FROM schema_version", &[])?.first().map_or(0, |r| r.int(0));
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
            let begin = b.begin();
            b.batch(begin)?;
            let applied = b.batch(&b.ddl(sql)).and_then(|_| b.execute("INSERT INTO schema_version (version) VALUES ($1)", &[Value::Int(i as i64 + 1)]));
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

    /// Runs `f` in one transaction: committed on `Ok`, rolled back on `Err`.
    pub fn tx<T, E: From<DbError>>(&self, f: impl FnOnce(&mut Tx) -> Result<T, E>) -> Result<T, E> {
        let mut guard = self.backend.lock().unwrap_or_else(|e| e.into_inner());
        let backend = guard.as_mut();
        let begin = backend.begin();
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
        assert_eq!(v[0].int(0), 3);
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
