//! High-level services composed on top of the [`crate::traits`] contracts.
//!
//! Services route a single user-facing operation across the storage and
//! infrastructure traits. They contain the business rules (which storage
//! tier holds the value, when to emit audit entries, validation order) and
//! are themselves stateless beyond their trait dependencies.

mod registry;

pub use registry::RegistryService;
