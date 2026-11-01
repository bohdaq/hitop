//! hitop sync server (AGPL-3.0-or-later): stores end-to-end encrypted workspace blobs, their
//! version vectors, device public keys and HPKE-wrapped workspace keys. It never holds a key that
//! decrypts anything; clients encrypt with `hitop-sync-crypto` (MIT). See THREAT_MODEL.md.

pub mod api;
pub mod auth;
pub mod blobs;
pub mod config;
pub mod db;
pub mod mail;
mod routes;
pub mod workspaces;

pub use api::{State, SyncApp};
pub use config::Config;
pub use db::Db;
pub use mail::Mailer;
