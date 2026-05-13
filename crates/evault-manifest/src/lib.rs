//! Parser and serializer for the project `evault.toml` manifest.
//!
//! [`FileManifestIo`] implements
//! [`evault_core::traits::ManifestIo`] by reading and writing TOML files
//! on disk. The on-disk format is a thin wire representation that maps
//! 1:1 onto [`evault_core::model::ManifestSnapshot`].
//!
//! # On-disk format
//!
//! ```toml
//! project_id = "550e8400-e29b-41d4-a716-446655440000"
//! name = "my-app"
//!
//! [vars]
//! NODE_ENV = "production"                     # inline literal
//! DATABASE_URL = { ref = "uuid-of-var" }      # reference to the registry
//!
//! [profiles.dev]
//! NODE_ENV = "development"
//! DATABASE_URL = { ref = "uuid-of-dev-db" }
//! ```
//!
//! ## Rules
//!
//! - `project_id` is the UUID of the project the manifest belongs to.
//! - `name` is the human-readable project name.
//! - `[vars]` lists the **default-profile** bindings.
//! - `[profiles.<name>]` lists overrides for a named profile.
//! - Each binding is either a literal string (`"..."`) or a table
//!   `{ ref = "<uuid>" }` referencing a variable in the central registry.
//! - The profile-name section is the binding's profile; bindings do not
//!   carry an inline `profile` field.
//!
//! # Atomic writes
//!
//! [`FileManifestIo`]'s `save` writes through a temporary sibling file
//! and renames into place so a process crash can never leave a
//! half-written manifest at `path`.
//!
//! # Examples
//!
//! ```
//! use evault_core::model::{
//!     BindingSource, ManifestBinding, ManifestSnapshot, Profile, ProjectId,
//! };
//! use evault_core::traits::ManifestIo;
//! use evault_manifest::FileManifestIo;
//! use std::path::PathBuf;
//!
//! let dir = tempfile::tempdir().unwrap();
//! let path = dir.path().join("evault.toml");
//!
//! let snap = ManifestSnapshot {
//!     project_id: ProjectId::new_v4(),
//!     name: "demo".into(),
//!     bindings: vec![ManifestBinding {
//!         key: "NODE_ENV".into(),
//!         source: BindingSource::Inline { value: "production".into() },
//!         profile: Profile::default_profile(),
//!     }],
//! };
//!
//! let io = FileManifestIo::new();
//! io.save(&path, &snap).unwrap();
//! let loaded = io.load(&path).unwrap();
//! assert_eq!(loaded.name, "demo");
//! ```
#![forbid(unsafe_code)]

mod io;
mod wire;

pub use io::FileManifestIo;
