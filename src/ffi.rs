#![allow(
    non_upper_case_globals,
    non_camel_case_types,
    non_snake_case,
    clippy::all
)]

mod bindings {
    #![allow(
        non_upper_case_globals,
        non_camel_case_types,
        non_snake_case,
        clippy::all
    )]

    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub use bindings::*;

#[cfg(test)]
mod tests {
    use super::{
        get_global_param_string, get_input_pair_index, get_param_index, set_config_bool,
        set_config_double, set_config_string, HAPropsSI, PropsSI,
    };
    use std::{
        ffi::{CStr, CString},
        os::raw::{c_char, c_int},
        sync::{Mutex, OnceLock},
    };

    const EXPECTED_BOILING_POINT_K: f64 = 373.124_295_8;
    const EXPECTED_TOLERANCE: f64 = 1e-3;

    fn test_guard() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|err| err.into_inner())
    }

    fn c_string(input: &str) -> CString {
        CString::new(input).expect("CoolProp strings must not contain interior NULs")
    }

    #[test]
    fn props_si_matches_expected() {
        let _guard = test_guard();
        let fluid = c_string("Water");
        let output = c_string("T");
        let name1 = c_string("P");
        let name2 = c_string("Q");

        let value = unsafe {
            PropsSI(
                output.as_ptr(),
                name1.as_ptr(),
                101_325.0,
                name2.as_ptr(),
                0.0,
                fluid.as_ptr(),
            )
        };

        assert!(
            (value - EXPECTED_BOILING_POINT_K).abs() < EXPECTED_TOLERANCE,
            "expected {:.6}, got {value}",
            EXPECTED_BOILING_POINT_K
        );
    }

    #[test]
    fn linkage_exposes_auxiliary_bindings() {
        let _guard = test_guard();

        let h_output = c_string("H");
        let t = c_string("T");
        let p = c_string("P");
        let r = c_string("R");
        let value = unsafe {
            HAPropsSI(
                h_output.as_ptr(),
                t.as_ptr(),
                300.0,
                p.as_ptr(),
                101_325.0,
                r.as_ptr(),
                0.5,
            )
        };
        assert!(value.is_finite());

        let input_pair = unsafe { get_input_pair_index(c_string("PT_INPUTS").as_ptr()) };
        assert!(input_pair >= 0);

        let param_index = unsafe { get_param_index(c_string("T").as_ptr()) };
        assert!(param_index >= 0);

        unsafe {
            set_config_bool(c_string("ENABLE_SUPERANCILLARIES").as_ptr(), true);
            set_config_bool(c_string("ENABLE_SUPERANCILLARIES").as_ptr(), false);
            set_config_double(c_string("SPINODAL_MINIMUM_DELTA").as_ptr(), 0.45);
            set_config_double(c_string("SPINODAL_MINIMUM_DELTA").as_ptr(), 0.5);
            set_config_string(
                c_string("FLOAT_PUNCTUATION").as_ptr(),
                c_string(".").as_ptr(),
            );
        }

        let mut buffer = vec![0 as c_char; 128];
        let key = c_string("version");
        let status = unsafe {
            get_global_param_string(key.as_ptr(), buffer.as_mut_ptr(), buffer.len() as c_int)
        };
        assert!(
            status >= 0,
            "get_global_param_string failed with status {status}"
        );

        let version = unsafe { CStr::from_ptr(buffer.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        assert!(
            !version.trim().is_empty(),
            "CoolProp version string is empty"
        );
    }
}
