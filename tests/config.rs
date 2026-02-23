#[path = "common/mod.rs"]
mod common;

use anyhow::Result;
use common::test_lock;
use coolprop::{
    get_config_bool, get_config_double, get_config_string, set_config_bool, set_config_double,
    set_config_string,
};

#[test]
fn set_config_wrappers_reject_embedded_nul() {
    let _guard = test_lock().lock().unwrap();
    assert!(set_config_string("bad\0key", "value").is_err());
    assert!(set_config_double("bad\0key", 1.23).is_err());
    assert!(set_config_bool("bad\0key", true).is_err());
    assert!(get_config_string("bad\0key").is_err());
    assert!(get_config_double("bad\0key").is_err());
    assert!(get_config_bool("bad\0key").is_err());
}

#[test]
fn set_config_wrappers_allow_updates() -> Result<()> {
    let _guard = test_lock().lock().unwrap();
    set_config_bool("NORMALIZE_GAS_CONSTANTS", false)?;
    set_config_double("SPINODAL_MINIMUM_DELTA", 0.5)?;
    set_config_string("FLOAT_PUNCTUATION", ".")?;
    let normalize = get_config_bool("NORMALIZE_GAS_CONSTANTS")?;
    let min_delta = get_config_double("SPINODAL_MINIMUM_DELTA")?;
    let punctuation = get_config_string("FLOAT_PUNCTUATION")?;
    assert!(!normalize);
    assert!(min_delta.is_finite());
    assert!(min_delta > 0.0);
    assert_eq!(punctuation, ".");
    Ok(())
}
