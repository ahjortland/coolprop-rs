use std::ffi::NulError;

use thiserror::Error;

/// Result type used throughout the `coolprop` crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can arise while interacting with CoolProp.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// CoolProp returned a non-zero error code with an accompanying message.
    #[error("CoolProp error {code}: {message}")]
    CoolProp { code: i64, message: String },

    #[error("CoolProp global error: {message}")]
    CoolPropGlobalError { message: String },

    /// CoolProp reported an unknown phase classification code.
    #[error("phase code {0} is not recognized by CoolProp")]
    UnknownPhaseCode(i64),

    /// The caller provided input that CoolProp rejected.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("{context} failed: {message}")]
    Computation { context: String, message: String },
    
    #[error("global parameter `{param}` query failed: {message}")]
    GlobalParameter { param: String, message: String },

    /// One of the supplied strings contained an interior NUL byte.
    #[error("embedded NUL byte in {label}")]
    EmbeddedNul {
        label: &'static str,
        #[source]
        source: NulError,
    },
}
