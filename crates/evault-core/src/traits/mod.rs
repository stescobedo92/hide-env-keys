//! Trait contracts for the IO and infrastructure concerns of `evault`.
//!
//! Every IO concern (storage, manifest parsing, process execution, scanning,
//! clock, id generation) is expressed as a trait so that the business logic
//! built on top of them can be tested against in-memory fakes (see
//! `evault-store-memory`) and so that backends can be swapped without
//! touching the core.

mod audit_sink;
mod clock;
mod id_gen;
mod manifest_io;
mod materializer;
mod metadata_store;
mod process_runner;
mod scanner;
mod secret_store;

pub use audit_sink::AuditSink;
pub use clock::{Clock, SystemClock};
pub use id_gen::{IdGenerator, UuidV4IdGenerator};
pub use manifest_io::ManifestIo;
pub use materializer::Materializer;
pub use metadata_store::MetadataStore;
pub use process_runner::{ProcessOutcome, ProcessRunner};
pub use scanner::{CodeScanner, ScanHit};
pub use secret_store::SecretStore;
