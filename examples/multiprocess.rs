use anyhow::{Context, Result, anyhow};
use coolprop::{AbstractState, InputPair, Param};
use std::{
    env,
    process::{Command, Stdio},
    time::Duration,
};

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("--worker") => {
            let id = args
                .next()
                .context("worker invocation missing identifier")?
                .parse::<usize>()
                .context("worker identifier is not a number")?;
            run_worker(id)
        }
        Some(flag) => Err(anyhow!("unexpected flag supplied to example: {flag}")),
        None => run_coordinator(),
    }
}

fn run_coordinator() -> Result<()> {
    let worker_count = 4_usize;
    let exe = env::current_exe().context("failed to locate current executable")?;
    println!("Launching {worker_count} worker processes...");

    let mut children = Vec::with_capacity(worker_count);

    for id in 0..worker_count {
        let mut cmd = Command::new(&exe);
        let child = cmd
            .arg("--worker")
            .arg(id.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("failed to spawn worker process {id}"))?;
        children.push((id, child));
    }

    for (id, child) in children {
        let output = child
            .wait_with_output()
            .with_context(|| format!("failed to collect output from worker {id}"))?;
        if !output.status.success() {
            return Err(anyhow!("worker {id} exited with status {}", output.status));
        }
        let stdout = String::from_utf8(output.stdout)
            .with_context(|| format!("worker {id} produced non-UTF-8 output"))?;
        println!("--- worker {id} output ---");
        print!("{stdout}");
    }

    println!("All worker processes finished successfully.");
    Ok(())
}

fn run_worker(id: usize) -> Result<()> {
    let mut state = AbstractState::new("HEOS", "Water")
        .with_context(|| format!("worker {id} failed to construct AbstractState"))?;

    let scenarios = [
        Scenario {
            pressure: 90_000.0 + 2_500.0 * id as f64,
            temperature: 280.0 + 5.0 * id as f64,
        },
        Scenario {
            pressure: 101_325.0 + 5_000.0 * id as f64,
            temperature: 295.0 + 2.5 * id as f64,
        },
        Scenario {
            pressure: 120_000.0 + 3_000.0 * id as f64,
            temperature: 340.0 + 3.0 * id as f64,
        },
    ];

    for scenario in scenarios {
        state
            .update(InputPair::PT, scenario.pressure, scenario.temperature)
            .with_context(|| {
                format!("worker {id} failed to update state for scenario {scenario:?}")
            })?;

        let density = state
            .get(Param::Dmass)
            .with_context(|| format!("worker {id} failed to read density"))?;
        let enthalpy = state
            .get(Param::Hmass)
            .with_context(|| format!("worker {id} failed to read enthalpy"))?;
        let phase = state.phase().with_context(|| {
            format!("worker {id} failed to determine phase for scenario {scenario:?}")
        })?;

        println!(
            "worker {id}: T = {temperature:.2} K, P = {pressure:.2} Pa -> phase {phase}, \
density = {density:.4} kg/m^3, enthalpy = {enthalpy:.2} J/kg",
            id = id,
            temperature = scenario.temperature,
            pressure = scenario.pressure,
            phase = phase
        );
    }

    // Stagger completion to make it easier to see concurrent stdout when run without capture.
    std::thread::sleep(Duration::from_millis(150 * id as u64));

    Ok(())
}

#[derive(Debug, Copy, Clone)]
struct Scenario {
    pressure: f64,
    temperature: f64,
}
