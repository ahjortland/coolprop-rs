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
    CoolProp {
        /// Error code returned by the CoolProp C API.
        code: i64,
        /// Human-readable error message reported by CoolProp.
        message: String,
    },

    /// CoolProp reported an error through the global `errstring` channel.
    #[error("CoolProp global error: {message}")]
    CoolPropGlobalError {
        /// Human-readable error message reported by CoolProp.
        message: String,
    },

    /// CoolProp reported an unknown phase classification code.
    #[error("phase code {0} is not recognized by CoolProp")]
    UnknownPhaseCode(i64),

    /// The caller provided input that CoolProp rejected.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// A high-level call failed with extra operation context.
    #[error("{context} failed: {message}")]
    Computation {
        /// Label describing the operation that failed.
        context: String,
        /// Human-readable error message reported by CoolProp.
        message: String,
    },

    /// Querying a global string parameter failed.
    #[error("global parameter `{param}` query failed: {message}")]
    GlobalParameter {
        /// Parameter key passed to CoolProp.
        param: String,
        /// Human-readable error message reported by CoolProp.
        message: String,
    },

    /// One of the supplied strings contained an interior NUL byte.
    #[error("embedded NUL byte in {label}")]
    EmbeddedNul {
        /// Human-readable description of the offending input field.
        label: &'static str,
        #[source]
        /// Original UTF-8 to C-string conversion error.
        source: NulError,
    },
}
