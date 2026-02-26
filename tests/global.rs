#[path = "common/mod.rs"]
mod common;

use common::test_lock;
use coolprop::{fluid_param_string, global_param_string, phase_si, set_reference_state};

#[test]
fn global_param_string_version_nonempty() {
    let _guard = test_lock().lock().unwrap();
    let version = global_param_string("version").expect("version should be available");
    assert!(
        !version.trim().is_empty(),
        "CoolProp version string should not be empty"
    );
}

#[test]
fn global_param_string_invalid_parameter_errors() {
    let _guard = test_lock().lock().unwrap();
    let err = global_param_string("__definitely_not_a_valid_global_param__")
        .expect_err("expected error for invalid global parameter");
    let msg = err.to_string();
    assert!(
        msg.contains("global parameter"),
        "unexpected error text: {msg}"
    );
}

#[test]
fn fluid_param_string_aliases_nonempty() {
    let _guard = test_lock().lock().unwrap();
    let aliases = fluid_param_string("Water", "aliases").expect("aliases should be available");
    assert!(!aliases.trim().is_empty());
}

#[test]
fn phase_si_returns_phase_label() {
    let _guard = test_lock().lock().unwrap();
    let phase = phase_si("P", 101_325.0, "T", 300.0, "Water").expect("phase query should succeed");
    assert!(
        phase.to_lowercase().contains("liquid"),
        "unexpected phase label: {phase}"
    );
}

#[test]
fn set_reference_state_accepts_default_reset() {
    let _guard = test_lock().lock().unwrap();
    set_reference_state("Water", "default")
        .expect("setting default reference state should succeed");
    set_reference_state("Water", "DEF").expect("setting DEF reference state should succeed");
}
