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
