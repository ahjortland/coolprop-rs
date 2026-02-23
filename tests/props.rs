#[path = "common/mod.rs"]
mod common;

use anyhow::Result;
use coolprop::{props_si, props1_si};

#[test]
fn props_si_returns_error_for_invalid_request() {
    let _guard = common::test_lock().lock().unwrap();
    let err = props_si("NotAProperty", "P", 101_325.0, "T", 300.0, "Water")
        .expect_err("expected failure for invalid output");
    assert!(
        err.to_string().contains("PropsSI("),
        "unexpected error message: {err}"
    );
}

#[test]
fn props_si_success_path() -> Result<()> {
    let _guard = common::test_lock().lock().unwrap();
    let h = props_si("Hmass", "P", 101_325.0, "T", 300.0, "Water")?;
    assert!(h.is_finite());
    Ok(())
}

#[test]
fn props1_si_success_path() -> Result<()> {
    let _guard = common::test_lock().lock().unwrap();
    let t_crit = props1_si("Tcrit", "Water")?;
    assert!(t_crit.is_finite());
    assert!(t_crit > 600.0);
    Ok(())
}
