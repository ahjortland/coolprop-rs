#[path = "common/mod.rs"]
mod common;

use anyhow::Result;
use common::test_lock;
use coolprop::{set_config_bool, set_config_double, set_config_string};

#[test]
fn set_config_wrappers_reject_embedded_nul() {
    let _guard = test_lock().lock().unwrap();
    assert!(set_config_string("bad\0key", "value").is_err());
    assert!(set_config_double("bad\0key", 1.23).is_err());
    assert!(set_config_bool("bad\0key", true).is_err());
}

#[test]
fn set_config_wrappers_allow_updates() -> Result<()> {
    let _guard = test_lock().lock().unwrap();
    set_config_bool("debug_mode", false)?;
    set_config_double("R_U", 8.314462618_153_24)?;
    set_config_string("backend_path", "")?;
    Ok(())
}
