use anyhow::Result;
use coolprop::{AbstractState, InputPair, Param, props_si};
use std::thread;

fn main() -> Result<()> {
    println!("*** Multithreaded CoolProp demo ***");
    println!("Spawning workers that independently query thermodynamic properties.");

    // Warm up the dynamic loader on the main thread so the shared library is fully initialized
    // before the workers start. This avoids redundant fluid-library loads under concurrency.
    {
        let _ = props_si("P", "T", 300.0, "Q", 0.0, "Water")?;
        let mut warm_state = AbstractState::new("HEOS", "Water")?;
        warm_state.update(InputPair::PT, 101_325.0, 300.0)?;
        let _ = warm_state.get(Param::Hmass)?;
    }

    let mut handles = Vec::new();
    for idx in 0..4 {
        handles.push(thread::spawn(move || worker(idx)));
    }

    for handle in handles {
        let (idx, temperature, pressure, enthalpy) = handle
            .join()
            .expect("thread panicked")
            .expect("CoolProp computation failed");
        println!("Thread #{idx}: T={temperature:.2} K, p={pressure:.2} Pa, h={enthalpy:.2} J/kg");
    }

    Ok(())
}

fn worker(idx: usize) -> Result<(usize, f64, f64, f64)> {
    let temperature = 280.0 + idx as f64 * 10.0;
    let density = 3_000.0 + idx as f64 * 250.0;

    let pressure = props_si("P", "T", temperature, "Dmolar", density, "Water")?;

    let mut state = AbstractState::new("HEOS", "Water")?;
    state.update(InputPair::PT, pressure, temperature)?;
    let enthalpy = state.get(Param::Hmass)?;

    Ok((idx, temperature, pressure, enthalpy))
}
