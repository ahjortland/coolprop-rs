#[path = "common/mod.rs"]
mod common;

use anyhow::Result;
use common::{assert_close, test_lock};
use coolprop::{AbstractState, InputPair, Param, Phase, props_si};
use static_assertions::{assert_impl_all, assert_not_impl_any};

assert_impl_all!(AbstractState: Send);
assert_not_impl_any!(AbstractState: Sync);

#[test]
fn basic_state_metadata() -> Result<()> {
    let _guard = test_lock().lock().unwrap();
    let mut state = AbstractState::new("HEOS", "R134a")?;

    let handle = state.handle();
    assert!(
        handle >= 0,
        "state handle should be non-negative, got {handle}"
    );

    let backend = state.backend_name()?;
    assert_eq!(backend, "HelmholtzEOSBackend");
    let fluids = state.fluid_names()?;
    assert_eq!(fluids, "R134a");

    state.update(InputPair::PT, 1.0e5, 300.0)?;

    let aliases = state.fluid_param_string("aliases")?;
    assert!(
        !aliases.trim().is_empty(),
        "fluid aliases should not be empty"
    );

    state.specify_phase(Phase::Gas)?;
    assert_eq!(state.phase()?, Phase::Gas);
    state.unspecify_phase()?;
    let automatic_phase = state.phase()?;
    assert_ne!(automatic_phase, Phase::NotImposed);

    Ok(())
}

#[test]
fn debug_includes_runtime_metadata() -> Result<()> {
    let _guard = test_lock().lock().unwrap();
    let state = AbstractState::new("HEOS", "R134a")?;
    let dbg = format!("{state:?}");
    assert!(
        dbg.contains("AbstractState"),
        "Debug output should identify the type"
    );
    assert!(
        dbg.contains("HelmholtzEOSBackend"),
        "Debug output should include backend metadata"
    );
    assert!(
        dbg.contains("R134a"),
        "Debug output should include fluid metadata"
    );
    Ok(())
}

#[test]
fn try_clone_reconstructs_state() -> Result<()> {
    let _guard = test_lock().lock().unwrap();
    let mut state = AbstractState::new("HEOS", "R32&R125")?;
    let fractions = [0.4, 0.6];
    state.set_fractions(&fractions)?;
    state.update(InputPair::PT, 3.0e5, 290.0)?;

    let mut cloned = state.try_clone()?;
    let cloned_fractions = cloned.mole_fractions()?;
    assert_eq!(cloned_fractions.len(), fractions.len());
    for (idx, &value) in cloned_fractions.iter().enumerate() {
        assert_close(
            value,
            fractions[idx],
            1e-9,
            1e-12,
            "cloned mole fraction retrieval",
        );
    }

    cloned.update(InputPair::PT, 3.0e5, 290.0)?;
    let cloned_pressure = cloned.pressure()?;
    assert_close(cloned_pressure, 3.0e5, 1e-12, 1e-3, "cloned state pressure");

    Ok(())
}

#[test]
fn update_and_retrieve_properties() -> Result<()> {
    let _guard = test_lock().lock().unwrap();
    let mut state = AbstractState::new("HEOS", "R134a")?;

    let pressure = 1.0e5;
    let temperature = 300.0;
    state.update(InputPair::PT, pressure, temperature)?;
    let hmass_state = state.get(Param::Hmass)?;
    let hmass_expected = props_si("Hmass", "P", pressure, "T", temperature, "R134a")?;
    assert_close(
        hmass_state,
        hmass_expected,
        1e-9,
        1e-6,
        "hmass from PT update",
    );

    let dmolar = props_si("Dmolar", "P", pressure, "T", temperature, "R134a")?;
    state.update_dmolar_t(dmolar, temperature)?;
    let pressure_state = state.pressure()?;
    let pressure_expected = props_si("P", "Dmolar", dmolar, "T", temperature, "R134a")?;
    assert_close(
        pressure_state,
        pressure_expected,
        1e-9,
        1e-3,
        "pressure from dmolar/t update shortcut",
    );

    state.update(InputPair::DmolarT, dmolar, temperature)?;
    let pressure_roundtrip = state.get(Param::P)?;
    assert_close(
        pressure_roundtrip,
        pressure,
        1e-9,
        1e-3,
        "pressure after explicit DmolarT update",
    );

    Ok(())
}

#[test]
fn saturation_queries() -> Result<()> {
    let _guard = test_lock().lock().unwrap();
    let mut state = AbstractState::new("HEOS", "R134a")?;
    let sat_temp = 260.0;

    state.update(InputPair::QT, 0.0, sat_temp)?;
    let p_liq = state.pressure()?;
    let keyed_liq = state.saturated_liquid_keyed_output(Param::P)?;
    assert_close(p_liq, keyed_liq, 1e-9, 1e-3, "saturated liquid pressure");
    let keyed_liq_temp = state.keyed_output_sat_state(Phase::Liquid, Param::T)?;
    assert_close(
        keyed_liq_temp,
        sat_temp,
        1e-9,
        1e-6,
        "saturated liquid temperature",
    );

    state.update(InputPair::QT, 1.0, sat_temp)?;
    let p_vap = state.pressure()?;
    let keyed_vap = state.saturated_vapor_keyed_output(Param::P)?;
    assert_close(p_vap, keyed_vap, 1e-9, 1e-3, "saturated vapor pressure");
    let keyed_vap_temp = state.keyed_output_sat_state(Phase::Gas, Param::T)?;
    assert_close(
        keyed_vap_temp,
        sat_temp,
        1e-9,
        1e-6,
        "saturated vapor temperature",
    );

    state.update(InputPair::QT, 0.5, sat_temp)?;
    let sat_derivative = state.first_saturation_deriv(Param::P, Param::T)?;
    assert!(
        sat_derivative.is_finite(),
        "first saturation derivative should be finite"
    );

    Ok(())
}

#[test]
fn derivative_queries() -> Result<()> {
    let _guard = test_lock().lock().unwrap();
    let mut state = AbstractState::new("HEOS", "R134a")?;

    state.update(InputPair::PT, 8.0e5, 320.0)?;
    let first_partial = state.first_partial_deriv(Param::Smolar, Param::T, Param::P)?;
    assert!(
        first_partial.is_finite(),
        "first partial derivative should be finite"
    );

    let second_partial =
        state.second_partial_deriv(Param::Smolar, Param::T, Param::P, Param::P, Param::T)?;
    assert!(
        second_partial.is_finite(),
        "second partial derivative should be finite"
    );

    state.update(InputPair::QT, 0.3, 260.0)?;
    match state.first_two_phase_deriv(Param::Hmolar, Param::T, Param::Q) {
        Ok(val) => assert!(val.is_finite(), "two-phase derivative should be finite"),
        Err(err) => {
            let msg = err.to_string();
            assert!(
                msg.contains("CoolProp error"),
                "unexpected first_two_phase_deriv error: {msg}"
            );
        }
    }
    match state.first_two_phase_deriv_splined(Param::Hmolar, Param::T, Param::Q, 0.1) {
        Ok(val) => assert!(
            val.is_finite(),
            "splined two-phase derivative should be finite"
        ),
        Err(err) => {
            let msg = err.to_string();
            assert!(
                msg.contains("CoolProp error"),
                "unexpected first_two_phase_deriv_splined error: {msg}"
            );
        }
    }
    match state.second_two_phase_deriv(Param::Hmolar, Param::T, Param::Q, Param::P, Param::Q) {
        Ok(val) => assert!(
            val.is_finite(),
            "second two-phase derivative should be finite"
        ),
        Err(err) => {
            let msg = err.to_string();
            assert!(
                msg.contains("CoolProp error"),
                "unexpected second_two_phase_deriv error: {msg}"
            );
        }
    }

    Ok(())
}

#[test]
fn fractions_and_fugacity() -> Result<()> {
    let _guard = test_lock().lock().unwrap();
    let mut state = AbstractState::new("HEOS", "R32&R125")?;
    let mass_fractions = [0.55, 0.45];
    state.set_mass_fractions(&mass_fractions)?;
    let current_mass = state.mass_fractions()?;
    assert_eq!(current_mass.len(), mass_fractions.len());
    let sum_mass: f64 = current_mass.iter().sum();
    assert_close(sum_mass, 1.0, 1e-6, 1e-9, "mass fractions sum");

    let fractions = [0.4, 0.6];
    state.set_fractions(&fractions)?;

    state.update(InputPair::PT, 3.0e5, 290.0)?;
    let current = state.mole_fractions()?;
    assert_eq!(current.len(), fractions.len());
    for (idx, &value) in current.iter().enumerate() {
        assert_close(
            value,
            fractions[idx],
            1e-9,
            1e-12,
            "mole fraction retrieval",
        );
    }

    state.update(InputPair::QT, 0.3, 260.0)?;
    let sat_liq = state.mole_fractions_sat_state(Phase::Liquid)?;
    assert_eq!(sat_liq.len(), 2);
    let sum_liq: f64 = sat_liq.iter().sum();
    assert_close(sum_liq, 1.0, 1e-6, 1e-9, "liquid saturation fractions sum");

    let sat_vap = state.mole_fractions_sat_state(Phase::Gas)?;
    assert_eq!(sat_vap.len(), 2);
    let sum_vap: f64 = sat_vap.iter().sum();
    assert_close(sum_vap, 1.0, 1e-6, 1e-9, "vapor saturation fractions sum");

    state.update(InputPair::PT, 4.0e5, 300.0)?;
    let f0 = state.get_fugacity(0)?;
    let phi0 = state.get_fugacity_coefficient(0)?;
    assert!(
        f0.is_finite() && f0 > 0.0,
        "component fugacity should be positive and finite"
    );
    assert!(
        phi0.is_finite(),
        "component fugacity coefficient should be finite"
    );

    Ok(())
}

#[test]
fn batch_updates() -> Result<()> {
    let _guard = test_lock().lock().unwrap();
    let mut state = AbstractState::new("HEOS", "R134a")?;

    let pressures = [1.0e5, 2.0e5, 3.0e5];
    let temperatures = [280.0, 300.0, 320.0];
    let len = pressures.len();
    let outputs = state.update_and_common_out(InputPair::PT, &pressures, &temperatures)?;

    for i in 0..len {
        assert_close(
            outputs.temperature[i],
            temperatures[i],
            1e-12,
            1e-9,
            "temperature array",
        );
        assert_close(
            outputs.pressure[i],
            pressures[i],
            1e-12,
            1e-3,
            "pressure array",
        );
        let expected_dmolar = props_si("Dmolar", "P", pressures[i], "T", temperatures[i], "R134a")?;
        assert_close(
            outputs.rhomolar[i],
            expected_dmolar,
            1e-9,
            1e-6,
            "rhomolar array",
        );
        let expected_hmolar = props_si("Hmolar", "P", pressures[i], "T", temperatures[i], "R134a")?;
        assert_close(
            outputs.hmolar[i],
            expected_hmolar,
            1e-9,
            1e-3,
            "hmolar array",
        );
        let expected_smolar = props_si("Smolar", "P", pressures[i], "T", temperatures[i], "R134a")?;
        assert_close(
            outputs.smolar[i],
            expected_smolar,
            1e-9,
            1e-3,
            "smolar array",
        );
    }

    let single_out = state.update_and_1_out(InputPair::PT, &pressures, &temperatures, Param::P)?;
    for (idx, &val) in single_out.iter().enumerate() {
        assert_close(val, pressures[idx], 1e-12, 1e-3, "single out pressure");
    }

    let [out1, out2, out3, out4, out5] = state.update_and_5_out(
        InputPair::PT,
        &pressures,
        &temperatures,
        [
            Param::T,
            Param::P,
            Param::Dmolar,
            Param::Hmolar,
            Param::Smolar,
        ],
    )?;

    for i in 0..len {
        assert_close(
            out1[i],
            temperatures[i],
            1e-12,
            1e-9,
            "five-out temperature",
        );
        assert_close(out2[i], pressures[i], 1e-12, 1e-3, "five-out pressure");
        assert_close(
            out3[i],
            outputs.rhomolar[i],
            1e-9,
            1e-6,
            "five-out rhomolar consistency",
        );
        assert_close(
            out4[i],
            outputs.hmolar[i],
            1e-9,
            1e-3,
            "five-out hmolar consistency",
        );
        assert_close(
            out5[i],
            outputs.smolar[i],
            1e-9,
            1e-3,
            "five-out smolar consistency",
        );
    }

    Ok(())
}

#[test]
fn envelope_spinodal_and_critical_points() -> Result<()> {
    let _guard = test_lock().lock().unwrap();
    let mut state = AbstractState::new("HEOS", "R32&R125")?;
    state.set_fractions(&[0.5, 0.5])?;

    state.build_phase_envelope("none")?;
    let envelope = state.phase_envelope()?;
    assert!(
        !envelope.temperature.is_empty(),
        "phase envelope should return data"
    );
    if envelope.temperature.len() > 1 {
        assert!(
            envelope.temperature.iter().any(|v| *v > 0.0),
            "phase envelope temperatures should contain data"
        );
    }
    assert_eq!(
        envelope.x.len(),
        2,
        "mixture should report two liquid-phase components"
    );
    assert_eq!(
        envelope.y.len(),
        2,
        "mixture should report two vapor-phase components"
    );
    for idx in 0..envelope.temperature.len() {
        let sum_liq: f64 = envelope.x.iter().map(|comp| comp[idx]).sum();
        let sum_vap: f64 = envelope.y.iter().map(|comp| comp[idx]).sum();
        assert_close(
            sum_liq,
            1.0,
            1e-6,
            1e-9,
            "phase envelope liquid fractions sum",
        );
        assert_close(
            sum_vap,
            1.0,
            1e-6,
            1e-9,
            "phase envelope vapor fractions sum",
        );
    }

    state.build_spinodal()?;
    let spinodal = state.spinodal_data()?;
    let valid_spinodal = spinodal
        .tau
        .iter()
        .zip(&spinodal.delta)
        .zip(&spinodal.m1)
        .filter(|((a, b), c)| a.is_finite() && b.is_finite() && c.is_finite())
        .count();
    assert!(
        valid_spinodal > 0,
        "spinodal data should contain finite entries"
    );

    let critical_points = state.critical_points()?;
    assert!(
        !critical_points.is_empty(),
        "should detect at least one critical point"
    );

    Ok(())
}

#[test]
fn cubic_parameter_mutators() -> Result<()> {
    let _guard = test_lock().lock().unwrap();
    let mut state = AbstractState::new("PR", "Methane&Ethane")?;
    state.set_fractions(&[0.5, 0.5])?;

    state.set_binary_interaction_double(0, 1, "kij", 0.05)?;
    state.set_cubic_alpha_c(0, "MC", 1.0, 0.5, 0.25)?;
    state.set_cubic_alpha_c(1, "MC", 0.9, 0.4, 0.2)?;
    state.set_fluid_parameter_double(0, "cm", 0.0)?;
    state.set_fluid_parameter_double(1, "cm", 0.0)?;

    state.update(InputPair::PT, 5.0e5, 320.0)?;
    let pressure = state.pressure()?;
    assert_close(pressure, 5.0e5, 1e-9, 1e-2, "pressure after cubic settings");

    Ok(())
}
