//! Where a request's time went: DNS, connect (TCP and TLS together), waiting for the first
//! byte, and downloading the body (HITOP_NEXT_UI.md, track B).
//!
//! reqwest has no per-request timing API, so the default client ([`crate::http`]) gets two
//! hooks that write [`Event`]s into one shared [`Recorder`]: a DNS resolver that times each
//! lookup, and a connector layer that times each new connection (its span encloses the DNS
//! lookup it caused). After a request, [`Recorder::phases`] takes the events that started
//! inside the request's window. reqwest's public API can't split TCP from TLS, so `connect`
//! is both.
//!
//! A request that found a pooled connection records no connect event: [`Timings::reused`].
//!
//! Attribution is exact for one request at a time (the runner, the CLI). Two requests in
//! flight at once on different hosts are told apart by host (DNS) and by which connection
//! enclosed that lookup; two at once to the same host, or to IP literals (no lookup), can
//! swap their connect times. Totals are always exact.
#![cfg(feature = "native")]

use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Instant;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};

pub use crate::model::Timings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Dns,
    Connect,
}

#[derive(Debug, Clone)]
struct Event {
    kind: Kind,
    /// The host name looked up (DNS events only).
    host: Option<String>,
    start: Instant,
    end: Instant,
}

/// Events from the client's hooks, shared by every request on that client.
#[derive(Debug, Default, Clone)]
pub struct Recorder {
    events: Arc<Mutex<Vec<Event>>>,
}

/// Old events are dropped once there are more than this many.
const KEEP: usize = 512;

fn ms(d: std::time::Duration) -> u64 {
    d.as_millis() as u64
}

impl Recorder {
    fn push(&self, e: Event) {
        let mut events = self.events.lock().unwrap_or_else(|p| p.into_inner());
        if events.len() >= KEEP {
            let cut = events.len() + 1 - KEEP / 2;
            events.drain(..cut);
        }
        events.push(e);
    }

    /// The phases of a request to `url`'s host (None when the URL has no host), see [`Recorder::phases`].
    pub fn phases_for(&self, url: &str, start: Instant, headers: Instant, done: Instant) -> Option<Timings> {
        let host = reqwest::Url::parse(url).ok()?.host_str()?.trim_start_matches('[').trim_end_matches(']').to_string();
        Some(self.phases(&host, start, headers, done))
    }

    /// The phases of a request to `host` that started at `start`, had its headers at
    /// `headers` and its body at `done`. The events it used are removed.
    pub fn phases(&self, host: &str, start: Instant, headers: Instant, done: Instant) -> Timings {
        let mut events = self.events.lock().unwrap_or_else(|p| p.into_inner());
        let inside = |e: &Event| e.start >= start && e.start <= headers;
        let dns_at = events.iter().position(|e| e.kind == Kind::Dns && inside(e) && e.host.as_deref().is_some_and(|h| h.eq_ignore_ascii_case(host)));
        let dns = dns_at.map(|i| events[i].clone());
        let conn_at = events.iter().position(|e| {
            e.kind == Kind::Connect && inside(e) && match &dns {
                // The connection that looked this host up encloses the lookup.
                Some(d) => e.start <= d.start && e.end >= d.end,
                None => true,
            }
        });
        let conn = conn_at.map(|i| events[i].clone());
        let mut gone: Vec<usize> = dns_at.into_iter().chain(conn_at).collect();
        gone.sort_unstable_by(|a, b| b.cmp(a));
        for i in gone {
            events.remove(i);
        }
        drop(events);

        let total = ms(done.saturating_duration_since(start));
        let download = ms(done.saturating_duration_since(headers));
        let dns_ms = dns.as_ref().map_or(0, |d| ms(d.end - d.start));
        let connect_ms = conn.as_ref().map_or(0, |c| ms(c.end - c.start).saturating_sub(dns_ms));
        // Whatever isn't DNS, connect or download is waiting (rounding lands here too).
        let wait = total.saturating_sub(download + dns_ms + connect_ms);
        Timings { dns_ms, connect_ms, wait_ms: wait, download_ms: download, total_ms: total, reused: conn.is_none() }
    }
}

/// A resolver that records how long each lookup took. On its own it looks names up like
/// hyper's default (the system's getaddrinfo on a blocking thread); [`TimedResolver::wrapping`]
/// times another resolver instead (hitop-server's proxy keeps its address guard).
pub struct TimedResolver {
    recorder: Recorder,
    inner: Option<Arc<dyn Resolve>>,
}

impl TimedResolver {
    pub fn new(recorder: Recorder) -> Self {
        TimedResolver { recorder, inner: None }
    }

    pub fn wrapping(recorder: Recorder, inner: Arc<dyn Resolve>) -> Self {
        TimedResolver { recorder, inner: Some(inner) }
    }
}

impl Resolve for TimedResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let recorder = self.recorder.clone();
        let host = name.as_str().to_string();
        let start = Instant::now();
        if let Some(inner) = &self.inner {
            let lookup = inner.resolve(name);
            return Box::pin(async move {
                let addrs = lookup.await?;
                recorder.push(Event { kind: Kind::Dns, host: Some(host), start, end: Instant::now() });
                Ok(addrs)
            });
        }
        Box::pin(async move {
            let lookup = host.clone();
            let addrs: Vec<SocketAddr> = tokio::task::spawn_blocking(move || (lookup.as_str(), 0).to_socket_addrs().map(|a| a.collect()))
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })??;
            recorder.push(Event { kind: Kind::Dns, host: Some(host), start, end: Instant::now() });
            let addrs: Addrs = Box::new(addrs.into_iter());
            Ok(addrs)
        })
    }
}

/// A connector layer that records how long each new connection took to open.
#[derive(Clone)]
pub struct TimedConnectLayer {
    recorder: Recorder,
}

impl TimedConnectLayer {
    pub fn new(recorder: Recorder) -> Self {
        TimedConnectLayer { recorder }
    }
}

impl<S> tower_layer::Layer<S> for TimedConnectLayer {
    type Service = TimedConnect<S>;
    fn layer(&self, inner: S) -> TimedConnect<S> {
        TimedConnect { inner, recorder: self.recorder.clone() }
    }
}

#[derive(Clone)]
pub struct TimedConnect<S> {
    inner: S,
    recorder: Recorder,
}

impl<S, R> tower_service::Service<R> for TimedConnect<S>
where
    S: tower_service::Service<R>,
    S::Future: Send + 'static,
    S::Response: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<S::Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), S::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: R) -> Self::Future {
        let start = Instant::now();
        let recorder = self.recorder.clone();
        let fut = self.inner.call(req);
        Box::pin(async move {
            let res = fut.await;
            if res.is_ok() {
                recorder.push(Event { kind: Kind::Connect, host: None, start, end: Instant::now() });
            }
            res
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    #[test]
    fn phases_add_up_to_the_total() {
        let r = Recorder::default();
        let t = Instant::now();
        r.push(Event { kind: Kind::Connect, host: None, start: at(t, 1), end: at(t, 40) });
        r.push(Event { kind: Kind::Dns, host: Some("api.example".into()), start: at(t, 2), end: at(t, 10) });
        let p = r.phases("API.example", t, at(t, 100), at(t, 130));
        assert_eq!(p, Timings { dns_ms: 8, connect_ms: 31, wait_ms: 61, download_ms: 30, total_ms: 130, reused: false });
        // The events were used up: the next request on the same connection is a reuse.
        let next = r.phases("api.example", at(t, 130), at(t, 150), at(t, 151));
        assert_eq!(next, Timings { dns_ms: 0, connect_ms: 0, wait_ms: 20, download_ms: 1, total_ms: 21, reused: true });
    }

    #[test]
    fn events_outside_the_window_or_for_another_host_are_ignored() {
        let r = Recorder::default();
        let t = at(Instant::now(), 1000);
        r.push(Event { kind: Kind::Dns, host: Some("a.example".into()), start: at(t, 5), end: at(t, 9) });
        r.push(Event { kind: Kind::Connect, host: None, start: at(t, 4), end: at(t, 20) });
        r.push(Event { kind: Kind::Dns, host: Some("b.example".into()), start: at(t, 6), end: at(t, 30) });
        r.push(Event { kind: Kind::Connect, host: None, start: at(t, 5), end: at(t, 50) });
        let b = r.phases("b.example", t, at(t, 60), at(t, 61));
        assert_eq!((b.dns_ms, b.connect_ms, b.reused), (24, 21, false), "b's connection encloses b's lookup");
        let a = r.phases("a.example", t, at(t, 60), at(t, 61));
        assert_eq!((a.dns_ms, a.connect_ms), (4, 12));
        let late = r.phases("a.example", at(t, 100), at(t, 110), at(t, 111));
        assert!(late.reused);
    }

    #[test]
    fn old_events_are_dropped() {
        let r = Recorder::default();
        let t = Instant::now();
        for _ in 0..(KEEP * 3) {
            r.push(Event { kind: Kind::Connect, host: None, start: t, end: t });
        }
        assert!(r.events.lock().unwrap().len() <= KEEP);
    }
}
