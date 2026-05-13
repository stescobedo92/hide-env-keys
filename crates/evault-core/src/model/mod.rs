//! Domain model: the data the rest of the workspace operates on.
//!
//! Every entity exposes a constructor for the happy path and accessors for its
//! fields. Mutation is opt-in via dedicated methods that preserve invariants
//! (validation, `updated_at` bumps, etc.) rather than exposing public fields.

mod audit;
mod manifest;
mod profile;
mod project;
mod var;

pub use audit::{AuditAction, AuditEntry, AuditId};
pub use manifest::{BindingSource, ManifestBinding, ManifestSnapshot};
pub use profile::Profile;
pub use project::{Project, ProjectId, ProjectVar};
pub use var::{Group, Var, VarFilter, VarId, VarKind};
