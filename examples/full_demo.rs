use anyhow::{Context, Result};
use coolprop::{
    AbstractState, InputPair, Param, Phase, global_param_string, ha_props_si, props_si,
};
use std::time::Instant;

fn main() -> Result<()> {
    let start = Instant::now();
    println!("**************** INFORMATION ***************");
    println!(
        "This example replicates the dev/scripts/example_generator.py script written by Ian Bell."
    );
    println!("CoolProp version: {}", global_param_string("version")?);
    println!(
        "CoolProp gitrevision: {}",
        global_param_string("gitrevision")?
    );
    println!("CoolProp Fluids: {}", global_param_string("FluidsList")?);

    println!("*********** HIGH LEVEL INTERFACE *****************");
    let water = AbstractState::new("HEOS", "Water").context("creating water AbstractState")?;
    let tcrit = water.get(Param::TCritical)?;
    println!("Critical temperature of water: {tcrit} K");

    let pressure = 101_325.0;
    let boiling = props_si("T", "P", pressure, "Q", 0.0, "Water")?;
    println!("Boiling temperature of water at 101325 Pa: {boiling} K");

    let t = 300.0;
    let phase = phase_string(&water, pressure, t)?;
    println!("Phase of water at 101325 Pa and 300 K: {phase}");

    let cp = props_si("C", "P", pressure, "T", t, "Water")?;
    println!("c_p of water at 101325 Pa and 300 K: {cp} J/kg/K");

    let cp_deriv = props_si("d(H)/d(T)|P", "P", pressure, "T", t, "Water")?;
    println!("c_p of water (using derivatives) at 101325 Pa and 300 K: {cp_deriv} J/kg/K");

    println!("*********** HUMID AIR PROPERTIES *****************");
    if let Err(err) = humid_air_demo() {
        println!("Humid air calculations unavailable: {err}");
    }

    println!("*********** INCOMPRESSIBLE FLUID AND BRINES *****************");
    let density = props_si("D", "T", 300.0, "P", pressure, "INCOMP::MEG-50%")?;
    println!("Density of 50% (mass) ethylene glycol/water at 300 K, 101325 Pa: {density} kg/m^3");
    let viscosity = props_si("V", "T", 350.0, "P", pressure, "INCOMP::TD12")?;
    println!("Viscosity of Therminol D12 at 350 K, 101325 Pa: {viscosity} Pa-s");

    println!("*********** REFPROP *****************");
    match refprop_demo() {
        Ok(()) => {}
        Err(err) => println!("REFPROP unavailable: {err}"),
    }

    println!("*********** TABULAR BACKENDS *****************");
    match AbstractState::new("BICUBIC&HEOS", "R245fa") {
        Ok(tab) => {
            tab.update(InputPair::PT, pressure, t)?;
            println!(
                "Mass density of refrigerant R245fa at 300 K, 101325 Pa: {} kg/m^3",
                tab.get(Param::Dmass)?
            );
        }
        Err(err) => println!("Tabular backend not available: {err}"),
    }

    println!("*********** SATURATION DERIVATIVES (LOW-LEVEL INTERFACE) ***************");
    let sat = AbstractState::new("HEOS", "R245fa")?;
    sat.update(InputPair::PQ, pressure, 0.0)?;
    let derivative = sat.first_saturation_deriv(Param::P, Param::T)?;
    println!("First saturation derivative: {derivative} Pa/K");

    println!("*********** LOW-LEVEL INTERFACE *****************");
    let mixture = AbstractState::new("HEOS", "Water&Ethanol")?;
    mixture.set_fractions(&[0.5, 0.5])?;
    mixture.update(InputPair::PQ, pressure, 1.0)?;
    println!(
        "Normal boiling point temperature of water and ethanol: {} K",
        mixture.get(Param::T)?
    );

    println!("*********** LOW-LEVEL INTERFACE (REFPROP) *****************");
    if let Err(err) = refprop_low_level() {
        println!("Skipping REFPROP low-level example: {err}");
    }

    println!("Example completed in {:?}", start.elapsed());
    Ok(())
}

fn phase_string(state: &AbstractState, pressure: f64, t: f64) -> Result<String> {
    state.update(InputPair::PT, pressure, t)?;
    let phase = state.phase()?;
    let label = match phase {
        Phase::Liquid => "liquid",
        Phase::Supercritical => "supercritical",
        Phase::SupercriticalGas => "supercritical gas",
        Phase::SupercriticalLiquid => "supercritical liquid",
        Phase::CriticalPoint => "critical point",
        Phase::Gas => "gas",
        Phase::TwoPhase => "two-phase",
        Phase::Unknown => "unknown",
        Phase::NotImposed => "not imposed",
    };
    Ok(label.to_string())
}

fn refprop_demo() -> Result<()> {
    let version = global_param_string("REFPROP_version")?;
    println!("REFPROP version: {version}");

    let refprop_state =
        AbstractState::new("REFPROP", "WATER").context("REFPROP backend not available")?;
    let tcrit = refprop_state.get(Param::TCritical)?;
    println!("Critical temperature of water: {tcrit} K");

    let boiling = props_si("T", "P", 101_325.0, "Q", 0.0, "REFPROP::WATER")?;
    println!("Boiling temperature of water at 101325 Pa: {boiling} K");
    let cp = props_si("C", "P", 101_325.0, "T", 300.0, "REFPROP::WATER")?;
    println!("c_p of water at 101325 Pa and 300 K: {cp} J/kg/K");

    Ok(())
}

fn refprop_low_level() -> Result<()> {
    let state = AbstractState::new("REFPROP", "METHANE&ETHANE")?;
    state.set_fractions(&[0.2, 0.8])?;
    state.update(InputPair::QT, 1.0, 120.0)?;
    let dmolar = state.get(Param::Dmolar)?;
    println!("Vapor molar density: {dmolar} mol/m^3");
    Ok(())
}

fn humid_air_demo() -> Result<()> {
    let pressure = 101_325.0;
    let w = ha_props_si("W", "T", 300.0, "P", pressure, "R", 0.5)?;
    println!("Humidity ratio of 50% rel. hum. air at 300 K, 101325 Pa: {w} kg_w/kg_da");
    let rh = ha_props_si("R", "T", 300.0, "P", pressure, "W", w)?;
    println!("Relative humidity from last calculation: {rh} (fractional)");
    Ok(())
}
