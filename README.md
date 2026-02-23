# coolprop

Rust-first bindings for the [CoolProp](https://github.com/CoolProp/CoolProp) thermophysical property library. The crate wraps CoolProp's C API with safe, idiomatic Rust and ships with a vendored build of the native library so projects can depend on it without extra glue code.

## Highlights
- High-level SI property queries through `props_si` and dedicated humid-air calculations via `ha_props_si`.
- Rich `AbstractState` interface for iterating on thermodynamic states, retrieving derivatives, working with mixtures, and accessing CoolProp internals when needed.
- Ergonomic error handling with a unified `coolprop::Result` and descriptive failure messages from the underlying library.
- Flexible build story: compile CoolProp from source (default) or link against an existing installation by toggling Cargo features or environment variables.

## Getting Started

### Requirements
- A recent stable Rust toolchain (via [rustup](https://rustup.rs/)).
- CMake and a C++14 compiler toolchain capable of building CoolProp. On macOS and Linux this usually means `cmake` plus `clang`/`gcc`; on Windows install the Build Tools for Visual Studio.

### Add the crate

```toml
[dependencies]
coolprop = "0.1"
```

Build and run a quick property query:

```rust
use coolprop::props_si;

fn main() -> coolprop::Result<()> {
    // Density of water at 300 K and 1 atm
    let density = props_si("Dmass", "T", 300.0, "P", 101_325.0, "Water")?;
    println!("Water density: {density:.2} kg/m^3");
    Ok(())
}
```

Run `cargo run --example full_demo` for a comprehensive walkthrough of the APIs.

## Working with AbstractState

Use `AbstractState` when you need an owned CoolProp backend with iterative updates, derivatives, or mixture handling:

```rust
use coolprop::{AbstractState, InputPair, Param};

fn radial_compressor_case() -> coolprop::Result<f64> {
    let mut state = AbstractState::new("HEOS", "R134a")?;
    state.update(InputPair::PT, 800_000.0, 280.0)?;
    Ok(state.get(Param::Hmass)?)
}
```

`AbstractState` automatically frees its CoolProp handle when dropped. Construct independent states inside each worker thread or process as shown in `examples/multithread.rs` and `examples/multiprocess.rs`.

## Configuration & Metadata

- Call `global_param_string` to inspect CoolProp metadata such as version strings, fluid lists, or the last error message.
- Adjust global settings with `set_config_bool`, `set_config_double`, and `set_config_string`. These change CoolProp-wide behavior, so apply them during initialization and avoid concurrent configuration from multiple threads.
- Point CoolProp at a local REFPROP installation with `set_refprop_path("/path/to/refprop")`.

## Building CoolProp

### Vendored build (default)

The crate enables the `vendored` feature by default, triggering a CMake build of the CoolProp sources bundled under `vendor/CoolProp`. Customize the build with environment variables:

| Variable | Meaning | Default |
|----------|---------|---------|
| `COOLPROP_SOURCE_DIR` | Use a different CoolProp checkout for the build. | `vendor/CoolProp` |
| `COOLPROP_SHARED` | `"ON"` for shared library, `"OFF"` for static archive. | `ON` |
| `COOLPROP_BUILD_TYPE` | CMake build type (`Release`, `Debug`, ...). | `Release` |

### Link against an existing installation

Disable the default feature when you already have CoolProp built on the system:

```toml
[dependencies]
coolprop = { version = "0.1", default-features = false }
```

Provide the necessary paths through environment variables before running `cargo build`:

| Variable | Purpose |
|----------|---------|
| `COOLPROP_LIB_PATH` | Absolute path to the CoolProp library (`.so`, `.dylib`, `.dll`, `.a`). |
| `COOLPROP_LIB_DIR` | Directory added to the link search path (required if `COOLPROP_LIB_PATH` is absent). |
| `COOLPROP_LIB_NAME` | Library link name (defaults to `CoolProp`). |
| `COOLPROP_INCLUDE_DIR` | Directory containing `CoolPropLib.h` for bindgen. |
| `COOLPROP_LINK_STATIC` | Truthy value (`1`, `true`, `on`) to force static linkage. |
| `COOLPROP_LINK_CXX` | Truthy value to always link the C++ runtime (auto-enabled for static builds). |
| `COOLPROP_CXX_STDLIB` | Override the C++ runtime library name (`stdc++`, `c++`, etc.). |

Unset or empty variables are ignored, letting vendored metadata take precedence when the feature is enabled.

## Examples & Tests
- `examples/full_demo.rs` mirrors the upstream CoolProp showcase and exercises most APIs.
- `examples/multithread.rs` demonstrates concurrent property lookups after a warm-up pass.
- `examples/multiprocess.rs` spawns multiple worker processes and shares the compiled library.
- `tests/abstract_state.rs`, `tests/config.rs`, and `tests/error.rs` cover regression scenarios for the safe wrappers.

Run `cargo test` to execute the suite, or `cargo run --example <name>` to explore specific examples.

## License

The Rust crate code is distributed under the same terms as the upstream CoolProp project. The vendored CoolProp sources are released under the MIT License; see `vendor/CoolProp/LICENSE` for details.
