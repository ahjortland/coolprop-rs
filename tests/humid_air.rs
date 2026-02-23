#[path = "common/mod.rs"]
mod common;

use anyhow::Result;
use common::test_lock;
use coolprop::ha_props_si;

#[test]
fn humidity_roundtrip_relative_humidity() -> Result<()> {
    let _guard = test_lock().lock().unwrap();
    let pressure = 101_325.0;
    let w = ha_props_si("W", "T", 300.0, "P", pressure, "R", 0.5)?;
    assert!(
        w.is_finite() && w > 0.0,
        "humidity ratio should be positive and finite, got {w}"
    );
    let rh = ha_props_si("R", "T", 300.0, "P", pressure, "W", w)?;
    assert!(
        (rh - 0.5).abs() < 1e-9,
        "expected round-trip relative humidity of 0.5, got {rh}"
    );
    Ok(())
}

#[test]
fn invalid_relative_humidity_range_errors() {
    let _guard = test_lock().lock().unwrap();
    // RH > 1 should be rejected by underlying correlations or result in non-finite outputs
    let err = ha_props_si("W", "T", 300.0, "P", 101_325.0, "R", 1.5)
        .expect_err("expected error for RH > 1.0");
    let msg = err.to_string();
    assert!(
        msg.contains("HAPropsSI"),
        "unexpected error message content: {msg}"
    );
}
