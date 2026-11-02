//! hitop sync server (AGPL-3.0-or-later): stores end-to-end encrypted workspace blobs, their
//! version vectors, device public keys and HPKE-wrapped workspace keys. It never holds a key that
//! decrypts anything; clients encrypt with `hitop-sync-crypto` (MIT). See THREAT_MODEL.md.

pub mod api;
pub mod audit;
pub mod auth;
pub mod blobs;
pub mod config;
pub mod db;
pub mod dbtools;
pub mod limits;
pub mod mail;
pub mod oidc;
pub mod orgs;
pub mod ratelimit;
mod routes;
pub mod teams;
pub mod tokens;
pub mod workspaces;

pub use api::{State, SyncApp};
pub use config::Config;
pub use db::Db;
pub use mail::Mailer;
