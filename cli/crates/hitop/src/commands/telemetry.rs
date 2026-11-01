//! `hitop telemetry on|off|status`, and the weekly ping itself: off until
//! `hitop telemetry on`, then at most once a week, in the background of
//! whatever command runs. https://hitop.rws8.tech/privacy.html
use std::sync::mpsc;
use std::time::Duration;

use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;

use hitop_core::telemetry::{date, endpoint, now, os, Surface, Telemetry, WEEK};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Subcommand)]
pub enum TelemetryCmd {
    /// Send an anonymous ping once a week: random install id, "cli", version, OS
    On,
    /// Send nothing (the default) and forget the install id
    Off,
    /// Show whether it is on and exactly what would be sent
    Status,
}

pub fn run(cmd: TelemetryCmd) -> Result<()> {
    let t = Telemetry::new(Surface::Cli)?;
    match cmd {
        TelemetryCmd::On => {
            t.set_enabled(true)?;
            println!("{} Usage ping on: at most once a week hitop sends the ping below. `hitop telemetry off` stops it.", "✓".green());
            show(&t);
        }
        TelemetryCmd::Off => {
            t.set_enabled(false)?;
            println!("{} Usage ping off: nothing is sent, and the install id is deleted.", "✓".green());
        }
        TelemetryCmd::Status => show(&t),
    }
    Ok(())
}

fn show(t: &Telemetry) {
    let state = t.state();
    let body = match t.payload(VERSION) {
        Some(p) => serde_json::to_string(&p).unwrap_or_default(),
        None => format!(r#"{{"id":"<a random id, made when you turn it on>","surface":"cli","version":"{VERSION}","os":"{}"}}"#, os()),
    };
    if state.enabled {
        println!("Usage ping: {}. At most once a week hitop sends:", "on".bold());
    } else {
        println!("Usage ping: {} (nothing is sent). After `hitop telemetry on`, at most once a week hitop would send:", "off".bold());
    }
    println!("  POST {}", endpoint());
    println!("  {body}");
    if state.enabled {
        match state.last_ping {
            Some(last) => println!("Last sent {} UTC; the next one goes out with a command run after {}.", date(last), date(last + WEEK)),
            None => println!("Not sent yet: the next hitop command sends it."),
        }
    }
    println!("{}", format!("Setting: {}. Privacy: https://hitop.rws8.tech/privacy.html", t.path().display()).dimmed());
}

/// Starts the ping in the background when it is on and due; `None` otherwise.
pub fn start() -> Option<mpsc::Receiver<()>> {
    let t = Telemetry::new(Surface::Cli).ok()?;
    if !t.due(now()) {
        return None;
    }
    let (done, wait) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = t.maybe_ping(VERSION, now());
        let _ = done.send(());
    });
    Some(wait)
}

/// Gives a started ping a moment to finish before the process exits.
pub fn finish(ping: Option<mpsc::Receiver<()>>) {
    if let Some(wait) = ping {
        let _ = wait.recv_timeout(Duration::from_secs(3));
    }
}
