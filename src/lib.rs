//! Safe, idiomatic Rust bindings for the CoolProp thermophysical property library.
//!
//! This crate wraps CoolProp's C API with Rust error handling and ownership semantics while
//! preserving broad access to the underlying functionality.
#![warn(missing_docs)]

#[allow(missing_docs)]
pub mod ffi;

mod abstract_state;
mod error;
mod ha_props;
mod indices;
mod props;

use std::{
    ffi::{CStr, CString, c_char},
    path::Path,
};

pub use abstract_state::{
    AbstractState, BatchCommonOutputs, CriticalPoint, PhaseEnvelope, SpinodalCurve,
};
pub use error::{Error, Result};
pub use ha_props::ha_props_si;
pub use indices::{InputPair, Param, Phase};
pub use props::{props_si, props1_si};

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

pub(crate) fn c_buf_to_string(buf: &[c_char]) -> String {
    let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), buf.len()) };
    match CStr::from_bytes_until_nul(bytes) {
        Ok(cstr) => cstr.to_string_lossy().into_owned(),
        Err(_) => String::from_utf8_lossy(bytes)
            .trim_end_matches('\0')
            .to_string(),
    }
}

pub(crate) fn coolprop_global_error(context: &str) -> Error {
    let message = global_param_string("errstring").unwrap_or_else(|_| "unknown error".into());
    Error::CoolPropGlobalError {
        message: format!("{context}: {message}"),
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
            // Protect against non-terminated writes from the C side.
            buffer[capacity - 1] = 0;
            let value = c_buf_to_string(&buffer);
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
            let err_len = err_buf.len();
            err_buf[err_len - 1] = 0;
            let message = c_buf_to_string(&err_buf);
            return Err(Error::GlobalParameter {
                param: param.to_string(),
                message,
            });
        }
        capacity *= 2;
    }
}

/// Retrieve a high-level fluid metadata field using CoolProp `get_fluid_param_string`.
pub fn fluid_param_string(fluid: &str, param: &str) -> Result<String> {
    let fluid_c = CString::new(fluid).map_err(|source| Error::EmbeddedNul {
        label: "fluid",
        source,
    })?;
    let param_c = CString::new(param).map_err(|source| Error::EmbeddedNul {
        label: "param",
        source,
    })?;
    let context = format!("get_fluid_param_string({fluid}, {param})");
    let required_len =
        unsafe { ffi::get_fluid_param_string_len(fluid_c.as_ptr(), param_c.as_ptr()) };
    if required_len < 0 {
        return Err(coolprop_global_error(&context));
    }

    let mut capacity = (required_len as usize + 1).max(256);
    loop {
        let mut buffer = vec![0 as c_char; capacity];
        let status = unsafe {
            ffi::get_fluid_param_string(
                fluid_c.as_ptr(),
                param_c.as_ptr(),
                buffer.as_mut_ptr(),
                capacity as i32,
            )
        };
        if status == 1 {
            buffer[capacity - 1] = 0;
            return Ok(c_buf_to_string(&buffer));
        }
        if capacity >= (1 << 20) {
            return Err(coolprop_global_error(&context));
        }
        capacity *= 2;
    }
}

/// Determine phase as a short string label using CoolProp `PhaseSI`.
pub fn phase_si(name1: &str, prop1: f64, name2: &str, prop2: f64, fluid: &str) -> Result<String> {
    let name1_c = CString::new(name1).map_err(|source| Error::EmbeddedNul {
        label: "name1",
        source,
    })?;
    let name2_c = CString::new(name2).map_err(|source| Error::EmbeddedNul {
        label: "name2",
        source,
    })?;
    let fluid_c = CString::new(fluid).map_err(|source| Error::EmbeddedNul {
        label: "fluid",
        source,
    })?;
    let context = format!("PhaseSI({name1}={prop1}, {name2}={prop2}, {fluid})");
    let mut capacity = 64usize;
    loop {
        let mut buffer = vec![0 as c_char; capacity];
        let status = unsafe {
            ffi::PhaseSI(
                name1_c.as_ptr(),
                prop1,
                name2_c.as_ptr(),
                prop2,
                fluid_c.as_ptr(),
                buffer.as_mut_ptr(),
                capacity as i32,
            )
        };
        if status == 1 {
            buffer[capacity - 1] = 0;
            return Ok(c_buf_to_string(&buffer));
        }
        if capacity >= 4096 {
            return Err(coolprop_global_error(&context));
        }
        capacity *= 2;
    }
}

/// Set the reference-state convention for a fluid (`"IIR"`, `"ASHRAE"`, `"NBP"`, `"DEF"`).
pub fn set_reference_state(fluid: &str, reference_state: &str) -> Result<()> {
    let reference_state = match reference_state.trim() {
        state if state.eq_ignore_ascii_case("default") || state.eq_ignore_ascii_case("def") => {
            "DEF"
        }
        state if state.eq_ignore_ascii_case("iir") => "IIR",
        state if state.eq_ignore_ascii_case("ashrae") => "ASHRAE",
        state if state.eq_ignore_ascii_case("nbp") => "NBP",
        state => state,
    };
    let fluid_c = CString::new(fluid).map_err(|source| Error::EmbeddedNul {
        label: "fluid",
        source,
    })?;
    let state_c = CString::new(reference_state).map_err(|source| Error::EmbeddedNul {
        label: "reference_state",
        source,
    })?;
    let status = unsafe { ffi::set_reference_stateS(fluid_c.as_ptr(), state_c.as_ptr()) };
    if status == 1 {
        Ok(())
    } else {
        Err(coolprop_global_error(&format!(
            "set_reference_state({fluid}, {reference_state})"
        )))
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
/// - `"ALTERNATIVE_REFPROP_PATH"`: Custom path to REFPROP library
/// - `"ALTERNATIVE_TABLES_DIRECTORY"`: Custom directory for tabular data
/// - `"FLOAT_PUNCTUATION"`: Decimal separator character used when parsing numeric strings
///
/// # Examples
///
/// ```rust
/// use coolprop::set_config_string;
///
/// # fn main() -> coolprop::Result<()> {
/// # if cfg!(cp_docs_rs) { return Ok(()); }
/// // Ensure dot decimal separator for parsing numeric strings
/// set_config_string("FLOAT_PUNCTUATION", ".")?;
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

/// Get a boolean configuration value by key.
pub fn get_config_bool(key: &str) -> Result<bool> {
    let key_c = CString::new(key).map_err(|source| Error::EmbeddedNul {
        label: "config key",
        source,
    })?;
    #[cfg(coolprop_has_get_config_bool)]
    {
        let mut value = false;
        let status = unsafe { ffi::get_config_bool(key_c.as_ptr(), &mut value) };
        if status == 1 {
            Ok(value)
        } else {
            Err(coolprop_global_error(&format!("get_config_bool({key})")))
        }
    }
    #[cfg(not(coolprop_has_get_config_bool))]
    {
        let _ = key_c;
        Err(Error::InvalidInput(
            "this CoolProp build does not expose get_config_bool".into(),
        ))
    }
}

/// Get a floating-point configuration value by key.
pub fn get_config_double(key: &str) -> Result<f64> {
    let key_c = CString::new(key).map_err(|source| Error::EmbeddedNul {
        label: "config key",
        source,
    })?;
    #[cfg(coolprop_has_get_config_double)]
    {
        let mut value = 0.0f64;
        let status = unsafe { ffi::get_config_double(key_c.as_ptr(), &mut value) };
        if status == 1 {
            Ok(value)
        } else {
            Err(coolprop_global_error(&format!("get_config_double({key})")))
        }
    }
    #[cfg(not(coolprop_has_get_config_double))]
    {
        let _ = key_c;
        Err(Error::InvalidInput(
            "this CoolProp build does not expose get_config_double".into(),
        ))
    }
}

/// Get a string configuration value by key.
pub fn get_config_string(key: &str) -> Result<String> {
    let key_c = CString::new(key).map_err(|source| Error::EmbeddedNul {
        label: "config key",
        source,
    })?;
    #[cfg(coolprop_has_get_config_string)]
    {
        let mut capacity = 256usize;
        loop {
            let mut buffer = vec![0 as c_char; capacity];
            let status = unsafe {
                ffi::get_config_string(key_c.as_ptr(), buffer.as_mut_ptr(), capacity as i32)
            };
            if status == 1 {
                buffer[capacity - 1] = 0;
                return Ok(c_buf_to_string(&buffer));
            }
            if capacity >= (1 << 20) {
                return Err(coolprop_global_error(&format!("get_config_string({key})")));
            }
            capacity *= 2;
        }
    }
    #[cfg(not(coolprop_has_get_config_string))]
    {
        let _ = key_c;
        Err(Error::InvalidInput(
            "this CoolProp build does not expose get_config_string".into(),
        ))
    }
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
/// - `"R_U_CODATA"`: Universal gas constant used by CoolProp
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
/// // Set spinodal tracing minimum delta
/// set_config_double("SPINODAL_MINIMUM_DELTA", 0.5)?;
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
/// Controls validation checks and optional CoolProp features.
///
/// # Thread Safety Warning
///
/// Configuration changes are **not thread-safe** and should only be performed during
/// initialization or when no other CoolProp operations are in progress.
///
/// # Common Parameters
///
/// - `"NORMALIZE_GAS_CONSTANTS"`: Normalize gas constants for mixtures
/// - `"ENABLE_SUPERANCILLARIES"`: Enable pure-fluid superancillary fast paths
/// - `"DONT_CHECK_PROPERTY_LIMITS"`: Disable range checking (use with caution)
///
/// # Examples
///
/// ```rust
/// use coolprop::set_config_bool;
///
/// # fn main() -> coolprop::Result<()> {
/// # if cfg!(cp_docs_rs) { return Ok(()); }
/// // Disable gas-constant normalization
/// set_config_bool("NORMALIZE_GAS_CONSTANTS", false)?;
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

/// Set the global path CoolProp uses to locate REFPROP files.
///
/// This is a convenience wrapper around
/// [`set_config_string`](crate::set_config_string) with the
/// `ALTERNATIVE_REFPROP_PATH` key.
pub fn set_refprop_path<P: AsRef<Path>>(p: P) -> Result<()> {
    set_config_string(
        "ALTERNATIVE_REFPROP_PATH",
        p.as_ref().to_string_lossy().as_ref(),
    )
}
