//! High-level thermodynamic property calculations for pure fluids and mixtures.
//!
//! This module provides the [`props_si`] function, which is the simplest way to calculate
//! thermodynamic properties using CoolProp. It mirrors the `PropsSI` function from CoolProp's
//! high-level API.

use crate::{Error, Result, check_finite_and_report_error, ffi};
use std::ffi::CString;

/// Calculate a thermodynamic property for a pure fluid or predefined mixture.
///
/// This is the primary high-level interface to CoolProp's property database. Given two
/// thermodynamic state variables (e.g., pressure and temperature), it returns the requested
/// output property.
///
/// # Arguments
///
/// * `output` - The property to calculate (see Output Properties below)
/// * `name1` - First input property name
/// * `prop1` - Value of first input property (SI units)
/// * `name2` - Second input property name  
/// * `prop2` - Value of second input property (SI units)
/// * `fluid` - Fluid identifier (see Fluid Specification below)
///
/// # Output Properties
///
/// Common output properties include:
///
/// | Name | Description | Unit |
/// |------|-------------|------|
/// | `T` | Temperature | K |
/// | `P` | Pressure | Pa |
/// | `Dmass` | Mass density | kg/m³ |
/// | `Dmolar` | Molar density | mol/m³ |
/// | `Hmass` | Mass specific enthalpy | J/kg |
/// | `Hmolar` | Molar specific enthalpy | J/mol |
/// | `Smass` | Mass specific entropy | J/(kg·K) |
/// | `Smolar` | Molar specific entropy | J/(mol·K) |
/// | `Umass` | Mass specific internal energy | J/kg |
/// | `Umolar` | Molar specific internal energy | J/mol |
/// | `Cpmass` | Mass specific constant-pressure heat capacity | J/(kg·K) |
/// | `Cvmass` | Mass specific constant-volume heat capacity | J/(kg·K) |
/// | `Q` | Vapor quality (mass fraction) | dimensionless (0-1) |
/// | `Phase` | Phase index | dimensionless |
/// | `viscosity` | Dynamic viscosity | Pa·s |
/// | `conductivity` | Thermal conductivity | W/(m·K) |
/// | `surface_tension` | Surface tension | N/m |
/// | `speed_of_sound` | Speed of sound | m/s |
///
/// **Derivatives** can be specified using the notation `d(A)/d(B)|C`:
/// - `d(Hmass)/d(T)|P` - Constant-pressure heat capacity
/// - `d(P)/d(T)|Dmolar` - Derivative of pressure with respect to temperature at constant density
///
/// # Input Properties
///
/// Valid input property pairs depend on the phase and fluid. Common pairs:
///
/// - `P`, `T` - Pressure and temperature (single-phase only)
/// - `P`, `Q` - Pressure and quality (two-phase)
/// - `T`, `Q` - Temperature and quality (two-phase)
/// - `Dmass`, `T` - Density and temperature
/// - `Hmass`, `P` - Enthalpy and pressure
/// - `Smass`, `P` - Entropy and pressure
///
/// **Note**: Not all property pairs are valid for all thermodynamic states. For example,
/// specifying `P` and `T` in the two-phase region is over-constrained (temperature and
/// pressure are not independent during phase change).
///
/// # Fluid Specification
///
/// ## Pure Fluids
///
/// Use the fluid name directly:
/// ```text
/// "Water", "R134a", "Nitrogen", "CarbonDioxide", "Methane"
/// ```
///
/// ## Predefined Mixtures
///
/// Some fluids can be specified with composition:
/// ```text
/// "Air"                          // Standard dry air composition
/// "R410A.mix"                    // Predefined refrigerant blend
/// "R407C.mix"
/// ```
///
/// ## Custom Mixtures
///
/// Specify components separated by `&` with optional mole fractions in brackets:
/// ```text
/// "Methane&Ethane"                    // Equal molar composition (default)
/// "Nitrogen[0.79]&Oxygen[0.21]"       // Specify mole fractions
/// "R32[0.697615]&R125[0.302385]"      // Refrigerant blend (R410A composition)
/// ```
///
/// ## Backend Specification
///
/// Prefix the fluid name with a backend identifier:
/// ```text
/// "HEOS::Water"              // Helmholtz EOS (default, high accuracy)
/// "REFPROP::Water"           // NIST REFPROP (requires installation)
/// "INCOMP::MEG-50%"          // 50% ethylene glycol solution
/// "INCOMP::TD12"             // Heat transfer fluid Therminol D12
/// "PR::Methane"              // Peng-Robinson cubic equation
/// "SRK::Ethane"              // Soave-Redlich-Kwong cubic equation
/// "BICUBIC&HEOS::R245fa"     // Tabular interpolation (faster)
/// ```
///
/// # Examples
///
/// ## Basic Property Calculation
///
/// ```rust
/// use coolprop::props_si;
///
/// # fn main() -> coolprop::Result<()> {
/// # if cfg!(cp_docs_rs) { return Ok(()); }
/// // Calculate density of water at 300 K and 101325 Pa
/// let density = props_si("Dmass", "T", 300.0, "P", 101_325.0, "Water")?;
/// println!("Water density: {:.2} kg/m³", density);
/// # Ok(())
/// # }
/// ```
///
/// ## Saturation Properties
///
/// ```rust
/// use coolprop::props_si;
///
/// # fn main() -> coolprop::Result<()> {
/// # if cfg!(cp_docs_rs) { return Ok(()); }
/// // Find boiling temperature of water at atmospheric pressure
/// let t_boil = props_si("T", "P", 101_325.0, "Q", 0.0, "Water")?;
/// println!("Boiling point: {:.2} K ({:.2} °C)", t_boil, t_boil - 273.15);
///
/// // Calculate enthalpy of vaporization
/// let h_liquid = props_si("Hmass", "P", 101_325.0, "Q", 0.0, "Water")?;
/// let h_vapor = props_si("Hmass", "P", 101_325.0, "Q", 1.0, "Water")?;
/// let h_fg = h_vapor - h_liquid;
/// println!("Enthalpy of vaporization: {:.0} J/kg", h_fg);
/// # Ok(())
/// # }
/// ```
///
/// ## Critical Properties
///
/// ```rust
/// use coolprop::props_si;
///
/// # fn main() -> coolprop::Result<()> {
/// # if cfg!(cp_docs_rs) { return Ok(()); }
/// // Get critical temperature (use any valid state point)
/// let t_crit = props_si("Tcrit", "T", 300.0, "Q", 0.0, "R134a")?;
/// let p_crit = props_si("pcrit", "T", 300.0, "Q", 0.0, "R134a")?;
/// println!("Critical point: {:.2} K, {:.0} Pa", t_crit, p_crit);
/// # Ok(())
/// # }
/// ```
///
/// ## Using Derivatives
///
/// ```rust
/// use coolprop::props_si;
///
/// # fn main() -> coolprop::Result<()> {
/// # if cfg!(cp_docs_rs) { return Ok(()); }
/// // Calculate heat capacity using derivative notation
/// let cp = props_si("d(Hmass)/d(T)|P", "P", 101_325.0, "T", 300.0, "Water")?;
/// println!("Heat capacity: {:.2} J/(kg·K)", cp);
///
/// // Compare with direct property
/// let cp_direct = props_si("Cpmass", "P", 101_325.0, "T", 300.0, "Water")?;
/// assert!((cp - cp_direct).abs() < 1e-6);
/// # Ok(())
/// # }
/// ```
///
/// ## Mixture Calculations
///
/// ```rust
/// use coolprop::props_si;
///
/// # fn main() -> coolprop::Result<()> {
/// # if cfg!(cp_docs_rs) { return Ok(()); }
/// // Natural gas approximation (90% methane, 10% ethane)
/// let mixture = "Methane[0.9]&Ethane[0.1]";
/// let density = props_si("Dmass", "T", 300.0, "P", 1e6, mixture)?;
/// println!("Natural gas density: {:.2} kg/m³", density);
/// # Ok(())
/// # }
/// ```
///
/// ## Alternative Backends
///
/// ```rust
/// use coolprop::props_si;
///
/// # fn main() -> coolprop::Result<()> {
/// # if cfg!(cp_docs_rs) { return Ok(()); }
/// // Use incompressible fluid database for ethylene glycol solution
/// let density = props_si("Dmass", "T", 300.0, "P", 101_325.0, "INCOMP::MEG-50%")?;
/// println!("50% glycol density: {:.2} kg/m³", density);
///
/// // Use cubic equation for faster (less accurate) calculation
/// let h = props_si("Hmass", "T", 300.0, "P", 1e5, "PR::Propane")?;
/// println!("Enthalpy (Peng-Robinson): {:.0} J/kg", h);
/// # Ok(())
/// # }
/// ```
///
/// # Thread Safety
///
/// This function is thread-safe after the first call completes (when the CoolProp library
/// is fully initialized). The first call to any CoolProp function may trigger fluid database
/// loading, which is internally synchronized.
///
/// For testing, consider using a mutex to serialize calls if you encounter issues with
/// concurrent access during initialization.
///
/// # Errors
///
/// Returns an error if:
///
/// - Invalid fluid name or fluid not found in database
/// - Invalid property names
/// - Invalid input property pair for the specified state
/// - State point is outside valid range for equation of state
/// - Numerical convergence failure in property calculation
/// - Any string parameter contains an embedded NUL byte
/// - The result is non-finite (NaN or infinite), indicating a calculation failure
///
/// The error message includes details from CoolProp's error string when available.
///
/// # Performance Notes
///
/// - For repeated calculations with the same fluid, consider using [`AbstractState`](crate::AbstractState)
///   to avoid recreating the internal state object
/// - The `BICUBIC&HEOS` backend creates interpolation tables for faster evaluation (at the cost
///   of some accuracy and initial table-building time)
/// - Derivative calculations may be slower than direct property evaluations
///
/// # References
///
/// - [CoolProp High-Level API](http://www.coolprop.org/coolprop/HighLevelAPI.html)
/// - [Fluid Properties](http://www.coolprop.org/fluid_properties/PurePseudoPure.html)
/// - [Mixture Properties](http://www.coolprop.org/fluid_properties/Mixtures.html)
pub fn props_si(
    output: &str,
    name1: &str,
    prop1: f64,
    name2: &str,
    prop2: f64,
    fluid: &str,
) -> Result<f64> {
    let context = format!("PropsSI({output}, {name1}={prop1}, {name2}={prop2}, {fluid})");
    let output_c = CString::new(output).map_err(|source| Error::EmbeddedNul {
        label: "output",
        source,
    })?;
    let name1_c = CString::new(name1).map_err(|source| Error::EmbeddedNul {
        label: "name1",
        source,
    })?;
    let name2_c = CString::new(name2).map_err(|source| Error::EmbeddedNul {
        label: "name2",
        source,
    })?;
    let fluid_c = CString::new(fluid).map_err(|source| Error::EmbeddedNul {
        label: "fluid",
        source,
    })?;
    let value = unsafe {
        ffi::PropsSI(
            output_c.as_ptr(),
            name1_c.as_ptr(),
            prop1,
            name2_c.as_ptr(),
            prop2,
            fluid_c.as_ptr(),
        )
    };
    check_finite_and_report_error(value, &context)
}

/// Calculate a state-independent fluid property using CoolProp `Props1SI`.
///
/// Typical outputs include constants such as critical temperature (`"Tcrit"`), critical pressure
/// (`"pcrit"`), or molar mass (`"molar_mass"`).
pub fn props1_si(output: &str, fluid: &str) -> Result<f64> {
    let context = format!("Props1SI({fluid}, {output})");
    let output_c = CString::new(output).map_err(|source| Error::EmbeddedNul {
        label: "output",
        source,
    })?;
    let fluid_c = CString::new(fluid).map_err(|source| Error::EmbeddedNul {
        label: "fluid",
        source,
    })?;
    let value = unsafe { ffi::Props1SI(fluid_c.as_ptr(), output_c.as_ptr()) };
    check_finite_and_report_error(value, &context)
}
