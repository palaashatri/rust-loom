//! Error types shared across the Loom Vision framework.

use std::fmt;
use std::io;

/// Errors that can be returned by Loom Vision operations.
///
/// Providers return this type from [`crate::CapabilityProvider::run`];
/// model-pack handling uses it for manifest, checksum, and I/O failures.
#[derive(Debug)]
pub enum VisionError {
    /// The provider does not accept the supplied input type.
    UnsupportedInput,
    /// The run was cancelled by the caller.
    Cancelled,
    /// A required provider, model, or backend is not available.
    ProviderUnavailable(String),
    /// A model pack (or its manifest) is malformed or unsafe.
    InvalidModelPack(String),
    /// A model file's SHA-256 does not match the value declared in the manifest.
    ChecksumMismatch,
    /// An I/O operation failed.
    Io(io::Error),
    /// Any other internal failure.
    Internal(String),
}

impl fmt::Display for VisionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VisionError::UnsupportedInput => {
                write!(f, "input type is not supported by this provider")
            }
            VisionError::Cancelled => write!(f, "operation was cancelled"),
            VisionError::ProviderUnavailable(msg) => {
                write!(f, "provider unavailable: {msg}")
            }
            VisionError::InvalidModelPack(msg) => write!(f, "invalid model pack: {msg}"),
            VisionError::ChecksumMismatch => write!(
                f,
                "checksum mismatch: file content does not match the declared SHA-256"
            ),
            VisionError::Io(err) => write!(f, "I/O error: {err}"),
            VisionError::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl std::error::Error for VisionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            VisionError::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<io::Error> for VisionError {
    fn from(err: io::Error) -> Self {
        VisionError::Io(err)
    }
}
