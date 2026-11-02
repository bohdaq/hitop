//! Operator tools on whole databases (HITOP Cloud C2): row counts and content hashes per table
//! (`db-stats`), and copying every table from one database to another (`migrate-db`, SQLite →
//! Postgres for the switch, Postgres → SQLite to roll it back without losing later writes).
//!
//! A table's hash is SHA-256 over the sorted SHA-256s of its rows; a row's bytes are its values in
//! column-name order, each tagged with its type and length. Rows are sorted in Rust, not by
//! `ORDER BY`, so SQLite and Postgres (which collate text differently) give the same hash for the
//! same rows: equal hashes after a copy mean every row arrived unchanged.

use serde_json::{json, Value as Json};
use sha2::{Digest, Sha256};

use crate::db::{Db, DbError, Row, Tx, Value};

/// One table's row count and content hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableStats {
    pub table: String,
    pub rows: i64,
    pub hash: String,
}

/// A database's schema version and every table's stats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stats {
    pub kind: String,
    pub schema: i64,
    pub tables: Vec<TableStats>,
}

impl Stats {
    pub fn to_json(&self) -> Json {
        json!({
            "database": self.kind,
            "schema": self.schema,
            "tables": self.tables.iter().map(|t| json!({ "table": t.table, "rows": t.rows, "sha256": t.hash })).collect::<Vec<_>>(),
        })
    }

    pub fn rows(&self) -> i64 {
        self.tables.iter().map(|t| t.rows).sum()
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn row_hash(row: &Row) -> [u8; 32] {
    let mut h = Sha256::new();
    for v in row.values() {
        match v {
            Value::Null => h.update([0u8]),
            Value::Int(i) => {
                h.update([1u8]);
                h.update(i.to_be_bytes());
            }
            Value::Text(s) => {
                h.update([2u8]);
                h.update((s.len() as u64).to_be_bytes());
                h.update(s.as_bytes());
            }
            Value::Bytes(b) => {
                h.update([3u8]);
                h.update((b.len() as u64).to_be_bytes());
                h.update(b);
            }
        }
    }
    h.finalize().into()
}

fn quote(name: &str) -> Result<String, DbError> {
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
        return Err(DbError(format!("unexpected table or column name {name:?}")));
    }
    Ok(format!("\"{name}\""))
}

/// The schema version of a database (0 for an empty one).
pub fn schema_of(tx: &mut Tx) -> Result<i64, DbError> {
    Ok(tx.query_one("SELECT COALESCE(MAX(version), 0) FROM schema_version", &[])?.map_or(0, |r| r.int(0)))
}

/// Calls `f` with every row of `table` (columns `cols`, as given), a workspace at a time when the
/// table has `workspace_id` (so the blobs table is never held in memory whole).
fn each_chunk(db: &Db, table: &str, cols: &[String], mut f: impl FnMut(Vec<Row>) -> Result<(), DbError>) -> Result<(), DbError> {
    let list = cols.iter().map(|c| quote(c)).collect::<Result<Vec<_>, _>>()?.join(", ");
    let t = quote(table)?;
    if cols.iter().any(|c| c == "workspace_id") {
        let groups = db.tx(|tx| tx.query(&format!("SELECT DISTINCT workspace_id FROM {t}"), &[]))?;
        for g in groups {
            let rows = db.tx(|tx| tx.query(&format!("SELECT {list} FROM {t} WHERE workspace_id = $1"), &[g.text(0).into()]))?;
            f(rows)?;
        }
        Ok(())
    } else {
        f(db.tx(|tx| tx.query(&format!("SELECT {list} FROM {t}"), &[]))?)
    }
}

fn sorted_columns(db: &Db, table: &str) -> Result<Vec<String>, DbError> {
    let mut cols = db.tx(|tx| tx.columns(table))?;
    if cols.is_empty() {
        return Err(DbError(format!("table {table} is missing")));
    }
    cols.sort();
    Ok(cols)
}

/// Row counts and content hashes of every table the migrations create.
pub fn stats(db: &Db) -> Result<Stats, DbError> {
    let (kind, schema) = db.tx(|tx| Ok::<_, DbError>((tx.kind().to_string(), schema_of(tx)?)))?;
    let mut tables = Vec::new();
    for table in Db::tables() {
        let cols = sorted_columns(db, &table)?;
        let mut hashes: Vec<[u8; 32]> = Vec::new();
        each_chunk(db, &table, &cols, |rows| {
            hashes.extend(rows.iter().map(row_hash));
            Ok(())
        })?;
        hashes.sort_unstable();
        let mut h = Sha256::new();
        for x in &hashes {
            h.update(x);
        }
        tables.push(TableStats { table, rows: hashes.len() as i64, hash: hex(&h.finalize()) });
    }
    Ok(Stats { kind, schema, tables })
}

/// What `copy` did: the stats of both sides after it (equal, or `copy` returned an error).
#[derive(Debug)]
pub struct CopyReport {
    pub source: Stats,
    pub target: Stats,
}

/// Copies every row of every table from `from` into `to`, in one transaction on `to`. Both are
/// migrated to this build's schema first (`Db::open` does that). Refused when `to` has any row in
/// any table, so it never merges into or overwrites a database in use. Afterwards the row counts
/// and content hashes of both sides must be equal, or the copy is reported as failed (the target
/// then holds the copy for inspection: drop it before trying again).
pub fn copy(from: &Db, to: &Db) -> Result<CopyReport, DbError> {
    let source_schema = from.tx(schema_of)?;
    let target_schema = to.tx(schema_of)?;
    if source_schema != target_schema || source_schema as usize != Db::schema_version() {
        return Err(DbError(format!("schema versions differ: source {source_schema}, target {target_schema}, this build {}", Db::schema_version())));
    }
    let tables = Db::tables();
    for table in &tables {
        let t = quote(table)?;
        let n = to.tx(|tx| tx.query_one(&format!("SELECT COUNT(*) FROM {t}"), &[]))?.map_or(0, |r| r.int(0));
        if n > 0 {
            return Err(DbError(format!("the target is not empty ({table} has {n} rows): migrate-db only copies into a new, empty database")));
        }
    }
    to.tx(|tx| {
        for table in &tables {
            let cols = sorted_columns(from, table)?;
            let mut theirs = tx.columns(table)?;
            theirs.sort();
            if theirs != cols {
                return Err(DbError(format!("table {table}: the columns differ ({} vs {})", cols.join(","), theirs.join(","))));
            }
            let list = cols.iter().map(|c| quote(c)).collect::<Result<Vec<_>, _>>()?.join(", ");
            let marks = (1..=cols.len()).map(|i| format!("${i}")).collect::<Vec<_>>().join(", ");
            let sql = format!("INSERT INTO {} ({list}) VALUES ({marks})", quote(table)?);
            each_chunk(from, table, &cols, |rows| {
                for row in rows {
                    tx.execute(&sql, row.values())?;
                }
                Ok(())
            })?;
        }
        Ok::<_, DbError>(())
    })?;
    let source = stats(from)?;
    let target = stats(to)?;
    if source.tables != target.tables {
        let diff: Vec<String> = source
            .tables
            .iter()
            .zip(&target.tables)
            .filter(|(a, b)| a != b)
            .map(|(a, b)| format!("{}: {} rows {} vs {} rows {}", a.table, a.rows, &a.hash[..12], b.rows, &b.hash[..12]))
            .collect();
        return Err(DbError(format!("the copy differs from the source: {}", diff.join("; "))));
    }
    Ok(CopyReport { source, target })
}

/// A consistent copy of a SQLite database in one file (`VACUUM INTO`), safe while the server
/// runs. The backup of a server that is still on SQLite.
pub fn snapshot_sqlite(db: &Db, out: &std::path::Path) -> Result<(), DbError> {
    if out.exists() {
        return Err(DbError(format!("{} exists", out.display())));
    }
    if db.kind() != "sqlite" {
        return Err(DbError("snapshot is for SQLite; back up Postgres with pg_dump (deploy/backup.sh)".into()));
    }
    db.execute_alone("VACUUM INTO $1", &[out.display().to_string().into()])?;
    Ok(())
}
