//! `loom-package` implements the Loom document package container.
//!
//! A `.loomdoc`, `.loomtable`, etc. is an inspectable ZIP archive with a
//! versioned manifest. This crate provides a small, dependency-free,
//! well-tested ZIP reader/writer plus the manifest model, checksums,
//! corruption detection, and safety limits against archive bombs and
//! malformed content.
//!
//! The format's specification (RFC-0006) lives in the separate `loom-spec` repository.

pub mod manifest;
pub mod zip;

pub use manifest::{Manifest, ManifestError, MimeType, PackageKind, SchemaVersion};
pub use zip::{ArchiveError, ArchiveLimits, Entry, PackageArchive};

/// Current schema version for the manifest format.
pub const MANIFEST_SEMVER: &str = "0.1.0";
