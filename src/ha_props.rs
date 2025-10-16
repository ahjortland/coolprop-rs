//! Psychrometric (humid air) property calculations.
//!
//! This module provides the [`ha_props_si`] function for calculating properties of moist air
//! mixtures. It implements psychrometric calculations following ASHRAE standards and is
//! essential for HVAC design, drying processes, and climate control applications.

use std::ffi::CString;

use crate::{check_finite_and_report_error, ffi, Error, Result};

/// Calculate psychrometric (humid air) properties.
///
/// This function computes properties of moist air (mixtures of dry air and water vapor) given
/// three input properties that define the thermodynamic state and composition. It is the
/// primary interface for psychrometric calculations in CoolProp.
///
/// # Arguments
///
/// * `output` - The property to calculate (see Output Properties below)
/// * `name1` - First input property name
/// * `prop1` - Value of first input property (SI units)
/// * `name2` - Second input property name
/// * `prop2` - Value of second input property (SI units)
/// * `name3` - Third input property name
/// * `prop3` - Value of third input property (SI units)
///
/// # Why Three Inputs?
///
/// Unlike pure substances, humid air is a binary mixture requiring **three** independent
/// properties to fully specify the state:
/// - One property for pressure (typically `P`)
/// - One property for temperature (typically `T`, `Tdb`, `Twb`, or `Tdp`)
/// - One property for composition (typically `W`, `R`, or `Vda`)
///
/// # Output Properties
///
/// | Name | Description | Unit |
/// |------|-------------|------|
/// | `T`, `Tdb` | Dry-bulb temperature | K |
/// | `Twb` | Wet-bulb temperature | K |
/// | `Tdp` | Dew-point temperature | K |
/// | `P` | Pressure | Pa |
/// | `W` | Humidity ratio (mass of water per mass of dry air) | kg_w/kg_da |
/// | `R`, `RH` | Relative humidity | dimensionless (0-1) |
/// | `H`, `Hda` | Mixture enthalpy per unit dry air | J/kg_da |
/// | `Hha` | Mixture enthalpy per unit humid air | J/kg_ha |
/// | `S`, `Sda` | Mixture entropy per unit dry air | J/(kg_da·K) |
/// | `Sha` | Mixture entropy per unit humid air | J/(kg_ha·K) |
/// | `V`, `Vda` | Mixture volume per unit dry air | m³/kg_da |
/// | `Vha` | Mixture volume per unit humid air | m³/kg_ha |
/// | `Y` | Mole fraction of water vapor | dimensionless |
/// | `psi_w` | Water vapor partial pressure | Pa |
/// | `mu` | Dynamic viscosity | Pa·s |
/// | `k` | Thermal conductivity | W/(m·K) |
///
/// # Input Properties
///
/// All properties in the output table can also be used as inputs. The most common input
/// combinations are:
///
/// - `T` (or `Tdb`), `P`, `R` - Dry-bulb temperature, pressure, relative humidity
/// - `T`, `P`, `W` - Dry-bulb temperature, pressure, humidity ratio
/// - `Twb`, `P`, `R` - Wet-bulb temperature, pressure, relative humidity
/// - `T`, `P`, `Tdp` - Dry-bulb temperature, pressure, dew-point temperature
/// - `H`, `P`, `R` - Enthalpy, pressure, relative humidity
///
/// # Units and Conventions
///
/// - Temperature: **Kelvin (K)** - add 273.15 to convert from °C
/// - Pressure: **Pascal (Pa)** - standard atmosphere is 101,325 Pa
/// - Relative humidity: **Fractional** (0.0 to 1.0, not percentage)
/// - Humidity ratio `W`: kg of water vapor per kg of **dry air** (not total mixture)
/// - Enthalpy `Hda`: J per kg of **dry air** (not per kg of humid air)
///
/// # Examples
///
/// ## Basic Psychrometric Calculation
///
/// ```rust
/// use coolprop::ha_props_si;
///
/// # fn main() -> coolprop::Result<()> {
/// # if cfg!(cp_docs_rs) { return Ok(()); }
/// // Calculate humidity ratio at 50% relative humidity
/// let humidity_ratio = ha_props_si(
///     "W",              // Output: humidity ratio
///     "T", 300.0,       // Dry-bulb temperature: 300 K (26.85°C)
///     "P", 101_325.0,   // Atmospheric pressure
///     "R", 0.5          // Relative humidity: 50%
/// )?;
/// println!("Humidity ratio: {:.6} kg_w/kg_da", humidity_ratio);
/// # Ok(())
/// # }
/// ```
///
/// ## Finding Dew Point
///
/// ```rust
/// use coolprop::ha_props_si;
///
/// # fn main() -> coolprop::Result<()> {
/// # if cfg!(cp_docs_rs) { return Ok(()); }
/// // Calculate dew point temperature
/// let dew_point = ha_props_si(
///     "Tdp",            // Output: dew-point temperature
///     "T", 298.15,      // Dry-bulb: 25°C
///     "P", 101_325.0,   // Atmospheric pressure
///     "R", 0.60         // Relative humidity: 60%
/// )?;
/// println!("Dew point: {:.2} K ({:.2} °C)",
///          dew_point, dew_point - 273.15);
/// # Ok(())
/// # }
/// ```
///
/// ## Wet-Bulb Temperature
///
/// ```rust
/// use coolprop::ha_props_si;
///
/// # fn main() -> coolprop::Result<()> {
/// # if cfg!(cp_docs_rs) { return Ok(()); }
/// // Calculate wet-bulb temperature from dry-bulb and humidity
/// let wet_bulb = ha_props_si(
///     "Twb",            // Output: wet-bulb temperature
///     "T", 308.15,      // Dry-bulb: 35°C
///     "P", 101_325.0,   // Atmospheric pressure
///     "R", 0.40         // Relative humidity: 40%
/// )?;
/// println!("Wet-bulb temperature: {:.2} °C", wet_bulb - 273.15);
/// # Ok(())
/// # }
/// ```
///
/// ## Enthalpy Calculation for HVAC
///
/// ```rust
/// use coolprop::ha_props_si;
///
/// # fn main() -> coolprop::Result<()> {
/// # if cfg!(cp_docs_rs) { return Ok(()); }
/// // Calculate specific enthalpy (used in cooling load calculations)
/// let enthalpy = ha_props_si(
///     "Hda",            // Enthalpy per kg of dry air
///     "T", 303.15,      // Indoor temp: 30°C
///     "P", 101_325.0,   // Atmospheric pressure
///     "R", 0.65         // Relative humidity: 65%
/// )?;
/// println!("Specific enthalpy: {:.0} J/kg_da", enthalpy);
///
/// // Calculate enthalpy per kg of humid air (less common)
/// let enthalpy_ha = ha_props_si(
///     "Hha",
///     "T", 303.15,
///     "P", 101_325.0,
///     "R", 0.65
/// )?;
/// println!("Specific enthalpy: {:.0} J/kg_ha", enthalpy_ha);
/// # Ok(())
/// # }
/// ```
///
/// ## Round-Trip Validation
///
/// ```rust
/// use coolprop::ha_props_si;
///
/// # fn main() -> coolprop::Result<()> {
/// # if cfg!(cp_docs_rs) { return Ok(()); }
/// let pressure = 101_325.0;
/// let temperature = 295.0; // K
/// let rh_target = 0.45;
///
/// // Calculate humidity ratio from relative humidity
/// let w = ha_props_si("W", "T", temperature, "P", pressure, "R", rh_target)?;
///
/// // Calculate relative humidity from humidity ratio (round trip)
/// let rh_calc = ha_props_si("R", "T", temperature, "P", pressure, "W", w)?;
///
/// assert!((rh_calc - rh_target).abs() < 1e-9,
///         "Round-trip relative humidity should match");
/// # Ok(())
/// # }
/// ```
///
/// ## Specific Volume and Density
///
/// ```rust
/// use coolprop::ha_props_si;
///
/// # fn main() -> coolprop::Result<()> {
/// # if cfg!(cp_docs_rs) { return Ok(()); }
/// // Calculate specific volume (m³/kg dry air)
/// let specific_volume = ha_props_si(
///     "Vda",
///     "T", 300.0,
///     "P", 101_325.0,
///     "R", 0.50
/// )?;
///
/// // Density is the reciprocal of specific volume
/// let density = 1.0 / specific_volume;
/// println!("Air density: {:.3} kg_da/m³", density);
/// # Ok(())
/// # }
/// ```
///
/// ## High Altitude Calculation
///
/// ```rust
/// use coolprop::ha_props_si;
///
/// # fn main() -> coolprop::Result<()> {
/// # if cfg!(cp_docs_rs) { return Ok(()); }
/// // Denver, Colorado: ~1600 m elevation, ~83 kPa pressure
/// let high_altitude_pressure = 83_000.0; // Pa
///
/// let w = ha_props_si(
///     "W",
///     "T", 298.15,              // 25°C
///     "P", high_altitude_pressure,
///     "R", 0.30                 // 30% RH
/// )?;
///
/// println!("Humidity ratio at altitude: {:.6} kg_w/kg_da", w);
/// # Ok(())
/// # }
/// ```
///
/// # Common Pitfalls
///
/// ## Relative Humidity Units
///
/// **Wrong** (percentage):
/// ```rust,ignore
/// ha_props_si("W", "T", 300.0, "P", 101_325.0, "R", 50.0) // Error!
/// ```
///
/// **Correct** (fractional):
/// ```rust,ignore
/// ha_props_si("W", "T", 300.0, "P", 101_325.0, "R", 0.50) // 50%
/// ```
///
/// ## Temperature Units
///
/// **Wrong** (Celsius):
/// ```rust,ignore
/// ha_props_si("W", "T", 25.0, "P", 101_325.0, "R", 0.5) // Error!
/// ```
///
/// **Correct** (Kelvin):
/// ```rust,ignore
/// let temp_celsius = 25.0;
/// let temp_kelvin = temp_celsius + 273.15;
/// ha_props_si("W", "T", temp_kelvin, "P", 101_325.0, "R", 0.5)
/// ```
///
/// ## Humidity Ratio Basis
///
/// The humidity ratio `W` is per kg of **dry air**, not per kg of humid air:
/// - Mass of dry air in humid air sample: `m_da = m_ha / (1 + W)`
/// - Mass of water vapor: `m_w = W * m_da`
///
/// # Physical Constraints
///
/// - Relative humidity must be between 0 and 1 (0% to 100%)
/// - Temperature must be above the dew point for the given humidity
/// - Very low temperatures (< 273 K / 0°C) may produce ice formation warnings
/// - Very high temperatures (> 350 K) may be outside the correlation range
/// - Pressure typically between 50 kPa and 200 kPa for standard correlations
///
/// # Thread Safety
///
/// This function is thread-safe after the first call completes. The first invocation may
/// trigger initialization of humid air property correlations.
///
/// # Errors
///
/// Returns an error if:
///
/// - Invalid property names
/// - Input values are outside physically meaningful ranges
/// - Thermodynamic state is inconsistent (e.g., dew point above dry-bulb temperature)
/// - Any string parameter contains an embedded NUL byte
/// - The result is non-finite (NaN or infinite)
/// - Relative humidity exceeds 1.0 or is negative
///
/// # References
///
/// - [ASHRAE Handbook - Fundamentals](https://www.ashrae.org/)
/// - [CoolProp Humid Air Documentation](http://www.coolprop.org/fluid_properties/HumidAir.html)
/// - Hyland and Wexler, "Formulations for the Thermodynamic Properties of the saturated
///   Phases of H₂O from 173.15 K to 473.15 K", ASHRAE Transactions, 1983
pub fn ha_props_si(
    output: &str,
    name1: &str,
    prop1: f64,
    name2: &str,
    prop2: f64,
    name3: &str,
    prop3: f64,
) -> Result<f64> {
    let context = format!("HAPropsSI({output:?}, ...)");
    let output = CString::new(output).map_err(|source| Error::EmbeddedNul {
        label: "output",
        source,
    })?;
    let name1 = CString::new(name1).map_err(|source| Error::EmbeddedNul {
        label: "name1",
        source,
    })?;
    let name2 = CString::new(name2).map_err(|source| Error::EmbeddedNul {
        label: "name2",
        source,
    })?;
    let name3 = CString::new(name3).map_err(|source| Error::EmbeddedNul {
        label: "name3",
        source,
    })?;
    let value = unsafe {
        (ffi::HAPropsSI)(
            output.as_ptr(),
            name1.as_ptr(),
            prop1,
            name2.as_ptr(),
            prop2,
            name3.as_ptr(),
            prop3,
        )
    };
    check_finite_and_report_error(value, &context)
}
