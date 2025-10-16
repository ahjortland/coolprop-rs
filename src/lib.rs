pub mod ffi;

mod abstract_state;
mod error;
mod indices;
mod props;
mod ha_props;

use std::{ffi::{c_char, CStr, CString}, path::Path};

pub use abstract_state::{
    AbstractState, BatchCommonOutputs, CriticalPoint, PhaseEnvelope, SpinodalCurve,
};
pub use error::{Error, Result};
pub use ha_props::ha_props_si;
pub use indices::{InputPair, Param, Phase};
pub use props::props_si;

pub(crate) fn check_finite_and_report_error(value: f64, context: &str) -> Result<f64> {
    if value.is_finite() {
        Ok(value)
    } else {
        let message = global_param_string("errstring").unwrap_or_else(|_| "unknown error".into());
        Err(Error::Computation {
            context: context.to_string(),
            message,
        })
    }
}

/// Retrieve a global parameter string from CoolProp.
///
/// This function queries CoolProp for various informational strings such as version numbers,
/// fluid lists, and configuration values.
///
/// # Common Parameters
///
/// - `"version"`: CoolProp version string (e.g., "6.4.1")
/// - `"gitrevision"`: Git commit hash of the CoolProp build
/// - `"FluidsList"`: Comma-separated list of all available pure and pseudo-pure fluids
/// - `"incompressible_list_pure"`: List of incompressible pure fluids
/// - `"incompressible_list_solution"`: List of incompressible solutions/brines
/// - `"REFPROP_version"`: REFPROP version (if available)
/// - `"errstring"`: Most recent error message from CoolProp
///
/// # Examples
///
/// ```rust
/// use coolprop::global_param_string;
///
/// # fn main() -> coolprop::Result<()> {
/// # if cfg!(cp_docs_rs) { return Ok(()); }
/// // Get CoolProp version
/// let version = global_param_string("version")?;
/// println!("CoolProp version: {}", version);
///
/// // List all available fluids
/// let fluids = global_param_string("FluidsList")?;
/// println!("Available fluids: {}", fluids);
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - The parameter name contains an embedded NUL byte
/// - The requested parameter does not exist or cannot be retrieved
/// - The buffer size is insufficient (automatically retried up to 1 MB)
pub fn global_param_string(param: &str) -> Result<String> {
    let key = CString::new(param).map_err(|source| Error::EmbeddedNul {
        label: "param",
        source,
    })?;
    let err_key = CString::new("errstring").expect("static string");

    let mut capacity: usize = 256;
    loop {
        let mut buffer = vec![0 as c_char; capacity];
        let status = unsafe {
            (ffi::get_global_param_string)(key.as_ptr(), buffer.as_mut_ptr(), capacity as i32)
        };
        if status == 1 {
            let value = unsafe { CStr::from_ptr(buffer.as_ptr()) }
                .to_string_lossy()
                .into_owned();
            return Ok(value);
        }
        if capacity >= (1 << 20) {
            let mut err_buf = vec![0 as c_char; 1024];
            unsafe {
                (ffi::get_global_param_string)(
                    err_key.as_ptr(),
                    err_buf.as_mut_ptr(),
                    err_buf.len() as i32,
                );
            }
            let message = unsafe { CStr::from_ptr(err_buf.as_ptr()) }
                .to_string_lossy()
                .into_owned();
            return Err(Error::GlobalParameter {
                param: param.to_string(),
                message,
            });
        }
        capacity *= 2;
    }
}

fn config_call<F>(action: F, context: &str) -> Result<()>
where
    F: FnOnce(),
{
    let _ = global_param_string("errstring");
    action();
    match global_param_string("errstring") {
        Ok(after) if after.is_empty() => Ok(()),
        Ok(after) => Err(Error::CoolPropGlobalError {
            message: format!("{context}: {after}"),
        }),
        Err(err) => Err(err),
    }
}

/// Set a string-valued configuration parameter in CoolProp.
///
/// Configuration parameters control global behavior such as debug mode, backend paths,
/// and numerical tolerances. Changes affect all subsequent CoolProp operations.
///
/// # Thread Safety Warning
///
/// Configuration changes are **not thread-safe** and should only be performed during
/// initialization or when no other CoolProp operations are in progress.
///
/// # Common Parameters
///
/// - `"backend_path"`: Directory path for alternative backend libraries
/// - `"ALTERNATIVE_REFPROP_PATH"`: Custom path to REFPROP library
/// - `"ALTERNATIVE_TABLES_DIRECTORY"`: Custom directory for tabular data
///
/// # Examples
///
/// ```rust
/// use coolprop::set_config_string;
///
/// # fn main() -> coolprop::Result<()> {
/// # if cfg!(cp_docs_rs) { return Ok(()); }
/// // Set custom backend path
/// set_config_string("backend_path", "/usr/local/lib/coolprop")?;
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - The key or value contains an embedded NUL byte
/// - The configuration parameter is invalid or read-only
/// - CoolProp rejects the value
pub fn set_config_string(key: &str, value: &str) -> Result<()> {
    let key_c = CString::new(key).map_err(|source| Error::EmbeddedNul {
        label: "config key",
        source,
    })?;
    let value_c = CString::new(value).map_err(|source| Error::EmbeddedNul {
        label: "config value",
        source,
    })?;
    let context = format!("set_config_string({key})");
    config_call(
        || unsafe {
            ffi::set_config_string(key_c.as_ptr(), value_c.as_ptr());
        },
        &context,
    )
}

/// Set a floating-point configuration parameter in CoolProp.
///
/// Adjusts numerical tolerances and physical constants used in CoolProp calculations.
///
/// # Thread Safety Warning
///
/// Configuration changes are **not thread-safe** and should only be performed during
/// initialization or when no other CoolProp operations are in progress.
///
/// # Common Parameters
///
/// - `"R_U"`: Universal gas constant (default: 8.314462618153)
/// - `"PHASE_ENVELOPE_STARTING_PRESSURE_PA"`: Initial pressure for phase envelope construction
/// - Various numerical tolerance parameters (see CoolProp documentation)
///
/// # Examples
///
/// ```rust
/// use coolprop::set_config_double;
///
/// # fn main() -> coolprop::Result<()> {
/// # if cfg!(cp_docs_rs) { return Ok(()); }
/// // Set universal gas constant (generally not needed)
/// set_config_double("R_U", 8.314462618153)?;
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - The key contains an embedded NUL byte
/// - The configuration parameter is invalid or read-only
/// - The value is outside acceptable bounds
pub fn set_config_double(key: &str, value: f64) -> Result<()> {
    let key_c = CString::new(key).map_err(|source| Error::EmbeddedNul {
        label: "config key",
        source,
    })?;
    let context = format!("set_config_double({key})");
    config_call(
        || unsafe {
            ffi::set_config_double(key_c.as_ptr(), value);
        },
        &context,
    )
}

/// Set a boolean configuration parameter in CoolProp.
///
/// Controls debug output, validation checks, and optional features.
///
/// # Thread Safety Warning
///
/// Configuration changes are **not thread-safe** and should only be performed during
/// initialization or when no other CoolProp operations are in progress.
///
/// # Common Parameters
///
/// - `"debug_mode"`: Enable verbose debugging output (default: false)
/// - `"NORMALIZE_GAS_CONSTANTS"`: Normalize gas constants for mixtures
/// - `"DONT_CHECK_PROPERTY_LIMITS"`: Disable range checking (use with caution)
///
/// # Examples
///
/// ```rust
/// use coolprop::set_config_bool;
///
/// # fn main() -> coolprop::Result<()> {
/// # if cfg!(cp_docs_rs) { return Ok(()); }
/// // Enable debug mode for troubleshooting
/// set_config_bool("debug_mode", true)?;
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - The key contains an embedded NUL byte
/// - The configuration parameter is invalid or read-only
pub fn set_config_bool(key: &str, value: bool) -> Result<()> {
    let key_c = CString::new(key).map_err(|source| Error::EmbeddedNul {
        label: "config key",
        source,
    })?;
    let context = format!("set_config_bool({key})");
    config_call(
        || unsafe {
            ffi::set_config_bool(key_c.as_ptr(), value);
        },
        &context,
    )
}

pub fn set_refprop_path<P: AsRef<Path>>(p: P) -> Result<()> {
    set_config_string(
        "ALTERNATIVE_REFPROP_PATH",
        p.as_ref().to_string_lossy().as_ref(),
    )
}
