//! Core types, traits, and services for the `evault` workspace.
//!
//! `evault-core` is intentionally backend-agnostic. It defines:
//!
//! - The **domain model**: variables, projects, profiles, audit entries.
//! - The **trait contracts** that every IO concern must implement, such as
//!   [`MetadataStore`](crate::traits::MetadataStore),
//!   [`SecretStore`](crate::traits::SecretStore),
//!   [`AuditSink`](crate::traits::AuditSink),
//!   [`ManifestIo`](crate::traits::ManifestIo),
//!   [`Materializer`](crate::traits::Materializer),
//!   [`CodeScanner`](crate::traits::CodeScanner),
//!   [`ProcessRunner`](crate::traits::ProcessRunner),
//!   [`Clock`](crate::traits::Clock) and
//!   [`IdGenerator`](crate::traits::IdGenerator).
//! - A few **value types** for handling secrets safely
//!   (see [`crypto::SecretString`] and [`crypto::MasterKey`]).
//!
//! Concrete implementations live in sibling crates such as
//! `evault-store-sqlcipher`, `evault-store-keyring`, `evault-store-memory`,
//! `evault-manifest`, `evault-runner`, `evault-materializer`, and
//! `evault-scanner-regex`.
//!
//! # Quick tour
//!
//! ```
//! use evault_core::model::{Var, Group, VarKind};
//!
//! let v = Var::new("DATABASE_URL", Group::User, VarKind::Secret);
//! assert_eq!(v.name(), "DATABASE_URL");
//! assert_eq!(v.group(), &Group::User);
//! ```
#![forbid(unsafe_code)]

pub mod crypto;
pub mod error;
pub mod model;
pub mod service;
pub mod traits;
