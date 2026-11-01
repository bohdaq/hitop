//! Outgoing mail (sign-in links). SMTP from the environment:
//!
//! - `SMTP_URL`: `smtp://user:password@host:587` (STARTTLS, required) or `smtps://...:465`
//!   (TLS from the start). Credentials are percent-decoded.
//! - `SMTP_TLS_SERVER_NAME`: the name the server's certificate is checked against when it differs
//!   from the host in `SMTP_URL` (for example `mail.rws8.tech` when connecting to 127.0.0.1).
//!   The certificate is always verified against the Mozilla roots (webpki-roots).
//! - `SMTP_FROM`: the From address (default `hitop <user@host-of-the-tls-name>`).
//! - `HITOP_SYNC_MAIL=log`: no SMTP; print sign-in mails to stderr (local testing only).

use std::sync::{Arc, Mutex};

use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::{Message, SmtpTransport, Transport};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mail {
    pub to: String,
    pub subject: String,
    pub body: String,
}

#[derive(Clone)]
pub enum Mailer {
    /// Not configured: email sign-in answers 503.
    Off,
    /// Prints to stderr.
    Log,
    /// Keeps mails in memory (tests).
    Memory(Arc<Mutex<Vec<Mail>>>),
    Smtp { transport: Box<SmtpTransport>, from: Mailbox },
}

impl Mailer {
    pub fn from_env() -> Result<Mailer, String> {
        if std::env::var("HITOP_SYNC_MAIL").as_deref() == Ok("log") {
            return Ok(Mailer::Log);
        }
        let Some(raw) = std::env::var("SMTP_URL").ok().filter(|s| !s.is_empty()) else {
            return Ok(Mailer::Off);
        };
        let url = url::Url::parse(&raw).map_err(|_| "SMTP_URL is not a URL".to_string())?;
        let host = url.host_str().ok_or("SMTP_URL has no host")?.to_string();
        let tls_name = std::env::var("SMTP_TLS_SERVER_NAME").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| host.clone());
        let params = TlsParameters::new(tls_name.clone()).map_err(|e| format!("SMTP TLS: {e}"))?;
        let (tls, default_port) = match url.scheme() {
            "smtp" => (Tls::Required(params), 587),
            "smtps" => (Tls::Wrapper(params), 465),
            other => return Err(format!("SMTP_URL scheme {other} (use smtp:// or smtps://)")),
        };
        let decode = |s: &str| url::form_urlencoded::parse(format!("x={s}").as_bytes()).next().map(|(_, v)| v.into_owned()).unwrap_or_default();
        let user = decode(url.username());
        let password = url.password().map(decode).unwrap_or_default();
        let mut builder = SmtpTransport::builder_dangerous(host).port(url.port().unwrap_or(default_port)).tls(tls).timeout(Some(std::time::Duration::from_secs(20)));
        if !user.is_empty() {
            builder = builder.credentials(Credentials::new(user.clone(), password));
        }
        let address = if user.contains('@') { user.clone() } else { format!("{}@{}", if user.is_empty() { "noreply" } else { &user }, tls_name.trim_start_matches("mail.")) };
        let from_default = format!("hitop <{address}>");
        let from_raw = std::env::var("SMTP_FROM").ok().filter(|s| !s.is_empty()).unwrap_or(from_default);
        let from: Mailbox = from_raw.parse().map_err(|_| format!("SMTP_FROM {from_raw:?} is not an address"))?;
        Ok(Mailer::Smtp { transport: Box::new(builder.build()), from })
    }

    pub fn enabled(&self) -> bool {
        !matches!(self, Mailer::Off)
    }

    /// Sends in the background for SMTP, so a slow mail server doesn't hold the request (and the
    /// answer takes the same time whether or not the address has an account).
    pub fn send(&self, mail: Mail) {
        match self {
            Mailer::Off => {}
            Mailer::Log => eprintln!("mail to {}: {}\n{}", mail.to, mail.subject, mail.body),
            Mailer::Memory(outbox) => outbox.lock().unwrap_or_else(|e| e.into_inner()).push(mail),
            Mailer::Smtp { transport, from } => {
                let transport = transport.as_ref().clone();
                let from = from.clone();
                std::thread::spawn(move || {
                    let message = mail.to.parse::<Mailbox>().map_err(|e| e.to_string()).and_then(|to| {
                        Message::builder().from(from).to(to).subject(mail.subject).body(mail.body).map_err(|e| e.to_string())
                    });
                    // The address is not logged: it's personal data.
                    match message.and_then(|m| transport.send(&m).map(|_| ()).map_err(|e| e.to_string())) {
                        Ok(()) => eprintln!("sign-in mail sent"),
                        Err(e) => eprintln!("sign-in mail failed: {e}"),
                    }
                });
            }
        }
    }
}
