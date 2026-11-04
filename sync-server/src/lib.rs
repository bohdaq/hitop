//! hitop sync server (AGPL-3.0-or-later): stores end-to-end encrypted workspace blobs, their
//! version vectors, device public keys and HPKE-wrapped workspace keys. It never holds a key that
//! decrypts client data; clients encrypt with `hitop-sync-crypto` (MIT). (Its own secrets, such as
//! per-org OIDC client secrets, it seals under its host key, hostkey.rs.) See THREAT_MODEL.md.

pub mod admin;
pub mod alerts;
pub mod anchors;
pub mod api;
pub mod audit;
pub mod auth;
pub mod billing;
pub mod blobs;
pub mod config;
pub mod db;
pub mod dbtools;
pub mod github;
pub mod hostkey;
pub mod library;
pub mod limits;
pub mod mail;
pub mod oidc;
pub mod orgs;
pub mod orgsso;
pub mod password;
pub mod ratelimit;
pub mod recovery;
mod routes;
pub mod runs;
pub mod scim;
pub mod shares;
pub mod support;
pub mod teams;
pub mod tokens;
pub mod workspaces;

pub use api::{State, SyncApp};
pub use config::Config;
pub use db::Db;
pub use mail::Mailer;
