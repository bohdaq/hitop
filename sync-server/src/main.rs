//! `hitop-sync-server`: listens where rws's settings say (`RWS_CONFIG_IP`, `RWS_CONFIG_PORT`,
//! default 127.0.0.1:7881 here) behind a TLS-terminating proxy. Settings: config.rs, mail.rs.

use hitop_sync_server::{alerts, audit, billing, dbtools, limits, runs, Config, Db, Mailer, State, SyncApp};
use rust_web_server::server::Server;

fn main() {
    // Defaults for the listener unless the environment sets them.
    for (name, value) in [("RWS_CONFIG_IP", "127.0.0.1"), ("RWS_CONFIG_PORT", "7881")] {
        if std::env::var_os(name).is_none() {
            std::env::set_var(name, value);
        }
    }
    let fail = |e: String| -> ! {
        eprintln!("hitop-sync-server: {e}");
        std::process::exit(2)
    };
    let config = Config::from_env().unwrap_or_else(|e| fail(e));
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str);
    // Commands that open databases of their own, before the configured one is opened.
    if command == Some("migrate-db") {
        std::process::exit(migrate_db(&args[1..], &config).unwrap_or_else(|e| fail(e)));
    }
    let mailer = Mailer::from_env().unwrap_or_else(|e| fail(e));
    if !mailer.enabled() {
        eprintln!("hitop-sync-server: SMTP_URL is not set, email sign-in is off");
    }
    let db = Db::open(&config.database).unwrap_or_else(|e| fail(e.to_string()));
    if config.oidc.is_some() {
        eprintln!("hitop-sync-server: SSO on (callback {})", config.public_url.join("v1/auth/sso/callback").map(|u| u.to_string()).unwrap_or_default());
    }
    let state = State::new(db, mailer, config).unwrap_or_else(|e| fail(e));
    // `hitop-sync-server audit-verify`: checks every audit chain in the database and exits.
    if command == Some("audit-verify") {
        let bad = print_audit_verify(&state).unwrap_or_else(|e| fail(e));
        std::process::exit(if bad == 0 { 0 } else { 1 });
    }
    // `hitop-sync-server db-stats [--json]`: row count and content hash of every table.
    if command == Some("db-stats") {
        let stats = dbtools::stats(&state.db).unwrap_or_else(|e| fail(e.to_string()));
        print_stats(&stats, args.iter().any(|a| a == "--json"));
        std::process::exit(0);
    }
    // `hitop-sync-server dump <dir>`: a backup (data file + manifest.json with its row counts and
    // hashes); deploy/backup.sh encrypts and uploads it. HITOP_SYNC_PG_DUMP: the pg_dump command.
    if command == Some("dump") {
        let Some(dir) = args.get(1) else { fail("usage: hitop-sync-server dump <empty directory>".into()) };
        let pg_dump = std::env::var("HITOP_SYNC_PG_DUMP").ok().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "pg_dump".into());
        let manifest = dbtools::dump(&state.db, &state.config.database, std::path::Path::new(dir), &pg_dump).unwrap_or_else(|e| fail(e.to_string()));
        println!("{manifest:#}");
        std::process::exit(0);
    }
    // `hitop-sync-server snapshot <file>`: a consistent copy of a SQLite database (backups).
    if command == Some("snapshot") {
        let Some(out) = args.get(1) else { fail("usage: hitop-sync-server snapshot <file>".into()) };
        dbtools::snapshot_sqlite(&state.db, std::path::Path::new(out)).unwrap_or_else(|e| fail(e.to_string()));
        println!("snapshot written to {out}");
        std::process::exit(0);
    }
    // `hitop-sync-server over-limits`: every org (or person, for devices) over a limit of its
    // plan; exit 0 when there is none (after migration 4 there is none).
    if command == Some("over-limits") {
        let over = state.db.tx(|tx| limits::over_limits(&state, tx)).unwrap_or_else(|e| fail(e.to_string()));
        for o in &over {
            println!("{}: {} {} of {}", o.org, o.metric.name(), o.used, o.max);
        }
        println!("{} over a limit", over.len());
        std::process::exit(if over.is_empty() { 0 } else { 1 });
    }
    // `hitop-sync-server set-plan <org> <plan> [seats]`: an org's plan until billing sets it.
    if command == Some("set-plan") {
        let [org, plan, rest @ ..] = &args[1..] else { fail("usage: hitop-sync-server set-plan <org> <plan> [seats]".into()) };
        let seats = rest.first().map(|s| s.parse::<i64>().unwrap_or_else(|_| fail("seats must be a number".into()))).unwrap_or(0);
        limits::set_plan(&state, org, plan, seats).unwrap_or_else(|e| fail(e));
        println!("{org}: {plan}, {seats} seat(s)");
        std::process::exit(0);
    }
    // `hitop-sync-server billing-events [--unmatched] [--json]`: the latest webhook deliveries.
    if command == Some("billing-events") {
        let events = billing::list_events(&state, args.iter().any(|a| a == "--unmatched")).unwrap_or_else(|e| fail(e.to_string()));
        if args.iter().any(|a| a == "--json") {
            println!("{}", serde_json::Value::from(events));
        } else {
            for e in &events {
                println!("{} {:<32} {:<18} org {} subscription {}{}", e["receivedAt"], e["event"].as_str().unwrap_or(""), e["outcome"].as_str().unwrap_or(""), e["org"].as_str().unwrap_or("-"), e["subscription"].as_str().unwrap_or("-"), if e["testMode"] == true { " (test mode)" } else { "" });
            }
            println!("{} deliveries", events.len());
        }
        std::process::exit(0);
    }
    // `hitop-sync-server billing-link <org> <subscription id>`: an unmatched subscription → org.
    if command == Some("billing-link") {
        let [org, sub, ..] = &args[1..] else { fail("usage: hitop-sync-server billing-link <org> <subscription id>".into()) };
        billing::link(&state, org, sub).unwrap_or_else(|e| fail(e));
        println!("{org}: subscription {sub} linked; its next webhook event applies");
        std::process::exit(0);
    }
    let app = SyncApp::new(state);
    // The audit retention purge: at start, then hourly.
    let purger = app.clone();
    std::thread::spawn(move || loop {
        match audit::purge(purger.state()) {
            Ok(0) => {}
            Ok(n) => eprintln!("hitop-sync-server: purged {n} audit rows past retention"),
            Err(e) => eprintln!("hitop-sync-server: audit purge: {e}"),
        }
        if let Err(e) = purger.state().db.tx(|tx| billing::expire(purger.state(), tx, None)) {
            eprintln!("hitop-sync-server: trial and period ends: {e}");
        }
        if let Err(e) = limits::purge(purger.state()) {
            eprintln!("hitop-sync-server: usage purge: {e}");
        }
        match runs::purge(purger.state()) {
            Ok(0) => {}
            Ok(n) => eprintln!("hitop-sync-server: purged {n} runs past their history"),
            Err(e) => eprintln!("hitop-sync-server: run purge: {e}"),
        }
        if let Err(e) = alerts::tick(purger.state()) {
            eprintln!("hitop-sync-server: missed-run check: {e}");
        }
        std::thread::sleep(std::time::Duration::from_secs(3600));
    });
    // Daily audit anchors (anchors.rs): hourly, the first an hour after start, so a start (an
    // upgrade, a database move and its row checks) writes nothing by itself.
    let anchorer = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
        match hitop_sync_server::anchors::daily(anchorer.state()) {
            Ok(0) => {}
            Ok(n) => eprintln!("hitop-sync-server: {n} daily audit anchors"),
            Err(e) => eprintln!("hitop-sync-server: audit anchors: {}", e.message),
        }
    });
    // Alert delivery: queued alerts (alerts.rs), every 5 s.
    let deliverer = app.clone();
    std::thread::spawn(move || loop {
        if let Err(e) = alerts::deliver_pending(deliverer.state()) {
            eprintln!("hitop-sync-server: alert delivery: {e}");
        }
        std::thread::sleep(std::time::Duration::from_secs(5));
    });
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio runtime");
    runtime.block_on(async {
        let (listener, pool) = Server::setup().expect("server setup");
        Server::run_tls(listener, pool, app).await;
    });
}

/// Prints every chain's state; returns the number of broken chains.
fn print_audit_verify(state: &State) -> Result<usize, String> {
    let chains = audit::verify_all(state).map_err(|e| e.to_string())?;
    let mut bad = 0;
    for (ws, v) in &chains {
        let name = if ws.is_empty() { "(accounts)" } else { ws.as_str() };
        match &v.problem {
            None => println!("{name}: ok, {} rows, head {} {}", v.checked, v.head_seq, v.head_hash),
            Some(p) => {
                bad += 1;
                println!("{name}: BROKEN: {p}");
            }
        }
    }
    println!("{} chains, {bad} broken", chains.len());
    Ok(bad)
}

fn print_stats(stats: &dbtools::Stats, as_json: bool) {
    if as_json {
        println!("{}", stats.to_json());
        return;
    }
    println!("{} database, schema {}", stats.kind, stats.schema);
    for t in &stats.tables {
        println!("{:<16} {:>8} rows  sha256 {}", t.table, t.rows, t.hash);
    }
    println!("{} rows in {} tables", stats.rows(), stats.tables.len());
}

/// `hitop-sync-server migrate-db --from <url> --to <url>`: copies every table into an empty
/// database, then compares row counts and content hashes and verifies every audit chain on the
/// target. Exit 0 when all of that holds, 1 when a check failed, 2 on usage or database errors.
fn migrate_db(args: &[String], config: &Config) -> Result<i32, String> {
    let usage = "usage: hitop-sync-server migrate-db --from <sqlite:path|postgres://...> --to <postgres://...|sqlite:path>";
    let mut from = None;
    let mut to = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--from" => from = it.next().cloned(),
            "--to" => to = it.next().cloned(),
            _ => return Err(usage.into()),
        }
    }
    let (Some(from), Some(to)) = (from, to) else { return Err(usage.into()) };
    if from == to {
        return Err("--from and --to are the same database".into());
    }
    if let Some(path) = to.strip_prefix("sqlite:") {
        if std::path::Path::new(path).exists() {
            return Err(format!("{path} exists: migrate-db only writes a new SQLite file"));
        }
    }
    if let Some(path) = from.strip_prefix("sqlite:") {
        if !std::path::Path::new(path).exists() {
            return Err(format!("{path} does not exist"));
        }
    }
    let source = Db::open(&from).map_err(|e| format!("--from: {e}"))?;
    let target = Db::open(&to).map_err(|e| format!("--to: {e}"))?;
    let report = match dbtools::copy(&source, &target) {
        Ok(r) => r,
        Err(e) if e.0.starts_with("the copy differs") => {
            println!("FAILED: {}", e.0);
            return Ok(1);
        }
        Err(e) => return Err(e.to_string()),
    };
    println!("copied {} rows in {} tables from {} to {}", report.source.rows(), report.source.tables.len(), report.source.kind, report.target.kind);
    print_stats(&report.target, false);
    println!("row counts and content hashes equal on both sides");
    let mut c = config.clone();
    c.database = to;
    let state = State::new(target, Mailer::Off, c)?;
    let bad = print_audit_verify(&state)?;
    Ok(if bad == 0 { 0 } else { 1 })
}
