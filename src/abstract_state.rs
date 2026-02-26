use crate::{
    Error, Result,
    indices::{Indices, InputPair, Param, Phase, global_indices},
};
use std::{
    cell::Cell,
    ffi::CString,
    fmt,
    marker::PhantomData,
    os::raw::{c_char, c_long},
    ptr,
};

const ERR_BUF_LEN: usize = 1024;
const DEFAULT_STR_BUF_LEN: usize = 1024;

/// High-level handle to CoolProp's `AbstractState`.
///
/// `AbstractState` owns a CoolProp backend object and exposes Rust-idiomatic wrappers for common
/// state updates, property queries, and configuration hooks. The type transparently manages the
/// underlying raw pointer and dispatches errors through `crate::Result`.
///
/// Typical workflow:
/// - Construct an instance with [`AbstractState::new`], giving the CoolProp backend (e.g., `"HEOS"`)
///   and fluid or mixture string.
/// - Call [`update`](Self::update) or one of the convenience setters to establish the thermodynamic
///   state.
/// - Retrieve properties with [`get`](Self::get) or phase-aware helpers such as
///   [`phase`](Self::phase).
/// - Optionally tune model parameters, impose phase constraints, or evaluate derivatives.
///
/// All methods return errors when CoolProp raises one; the message from CoolProp is included in the
/// [`crate::Error`]. Each instance owns its handle and releases it automatically when dropped.
///
/// # Threading
///
/// `AbstractState` is `Send` but not `Sync`. Move state objects between threads if needed, but do
/// not share a single instance concurrently.
pub struct AbstractState {
    indices: &'static Indices,
    handle: c_long,
    // CoolProp state objects are not safe to share across threads concurrently.
    // This keeps `Send` while preventing `Sync`.
    _not_sync: PhantomData<Cell<()>>,
}

#[derive(Debug, Clone, PartialEq)]
/// Outputs returned by [`AbstractState::update_and_common_out`].
pub struct BatchCommonOutputs {
    /// Temperature at each sampled input state, in kelvin.
    pub temperature: Vec<f64>,
    /// Pressure at each sampled input state, in pascals.
    pub pressure: Vec<f64>,
    /// Molar density at each sampled input state, in mol/m^3.
    pub rhomolar: Vec<f64>,
    /// Molar enthalpy at each sampled input state, in J/mol.
    pub hmolar: Vec<f64>,
    /// Molar entropy at each sampled input state, in J/(mol*K).
    pub smolar: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq)]
/// Full phase-envelope data extracted from CoolProp.
pub struct PhaseEnvelope {
    /// Saturation temperature coordinates, in kelvin.
    pub temperature: Vec<f64>,
    /// Saturation pressure coordinates, in pascals.
    pub pressure: Vec<f64>,
    /// Saturated-liquid molar density branch, in mol/m^3.
    pub rhomolar_liq: Vec<f64>,
    /// Saturated-vapor molar density branch, in mol/m^3.
    pub rhomolar_vap: Vec<f64>,
    /// Liquid composition matrix indexed as `x[component][point]`.
    pub x: Vec<Vec<f64>>,
    /// Vapor composition matrix indexed as `y[component][point]`.
    pub y: Vec<Vec<f64>>,
}

#[derive(Debug, Clone, PartialEq)]
/// Spinodal-curve sample points from CoolProp.
pub struct SpinodalCurve {
    /// Reduced inverse temperature `tau = Tc / T`.
    pub tau: Vec<f64>,
    /// Reduced density `delta = rho / rho_c`.
    pub delta: Vec<f64>,
    /// Leading eigenvalue along the spinodal track.
    pub m1: Vec<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
/// Critical point candidate returned by CoolProp for mixtures.
pub struct CriticalPoint {
    /// Temperature of the critical point, in kelvin.
    pub temperature: f64,
    /// Pressure of the critical point, in pascals.
    pub pressure: f64,
    /// Molar density of the critical point, in mol/m^3.
    pub rhomolar: f64,
    /// Stability flag reported by CoolProp.
    pub stable: bool,
}

impl AbstractState {
    /// Create a new CoolProp state object for the selected backend and fluid.
    ///
    /// `backend` is the CoolProp backend (such as `"HEOS"` or `"REFPROP"`), while `fluid` is the
    /// working fluid identifier or mixture string accepted by CoolProp. Both strings must be free
    /// of interior NUL bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if either string contains a NUL byte or CoolProp fails to construct the state.
    pub fn new(backend: &str, fluid: &str) -> Result<Self> {
        let indices = global_indices()?;
        let backend = CString::new(backend).map_err(|source| Error::EmbeddedNul {
            label: "backend",
            source,
        })?;
        let fluid = CString::new(fluid).map_err(|source| Error::EmbeddedNul {
            label: "fluid",
            source,
        })?;
        let handle = call_with_error(|err, msg, len| unsafe {
            crate::ffi::AbstractState_factory(backend.as_ptr(), fluid.as_ptr(), err, msg, len)
        })?;

        Ok(Self {
            indices,
            handle,
            _not_sync: PhantomData,
        })
    }

    /// Attempt to clone this state by reconstructing a fresh backend instance.
    ///
    /// CoolProp does not expose a native clone operation through its C API, so this method
    /// retrieves backend/fluid metadata and constructs a new state handle with the same
    /// configuration. When mole fractions are available, they are copied to the new state.
    pub fn try_clone(&self) -> Result<Self> {
        let backend = self.backend_name()?;
        let fluid = self.fluid_names()?;
        let mut cloned = match Self::new(&backend, &fluid) {
            Ok(state) => state,
            Err(initial_err) => {
                let normalized_fluid = fluid.replace(',', "&");
                if normalized_fluid == fluid {
                    return Err(initial_err);
                }
                Self::new(&backend, &normalized_fluid)?
            }
        };

        if let Ok(fractions) = self.mole_fractions() {
            let _ = cloned.set_fractions(&fractions);
        }

        Ok(cloned)
    }

    /// Raw CoolProp handle for advanced FFI integrations.
    ///
    /// Most users should rely on the safe wrappers; this accessor exists so that external callers
    /// can bridge to additional CoolProp entry points not yet covered by this crate.
    #[inline]
    #[must_use = "the raw handle is only useful if it is consumed by downstream FFI code"]
    pub fn handle(&self) -> c_long {
        self.handle
    }

    /// Update the thermodynamic state with the given CoolProp input pair.
    ///
    /// The `pair` determines which two intensive properties are being supplied (`v1`, `v2`).
    /// Values are forwarded directly to CoolProp; units must match the expectations of the selected
    /// backend. Convenience helpers such as [`update_dmolar_t`](Self::update_dmolar_t) delegate to
    /// this method.
    ///
    /// # Errors
    ///
    /// Propagates CoolProp errors (invalid pair for current phase, out-of-range inputs, etc.).
    #[inline]
    pub fn update(&mut self, pair: InputPair, v1: f64, v2: f64) -> Result<()> {
        let id = self.indices.id_of_pair(pair);
        call_with_error(|err, msg, len| unsafe {
            crate::ffi::AbstractState_update(self.handle, id, v1, v2, err, msg, len);
        })
    }

    /// Retrieve a scalar property identified by [`Param`].
    ///
    /// The state must be up to date before calling this method. Many `Param` variants refer to
    /// mass- or molar-specific values; ensure that downstream calculations use consistent bases.
    ///
    /// # Errors
    ///
    /// Returns the underlying CoolProp error if the property cannot be computed (e.g., outside the
    /// model's domain).
    #[inline]
    pub fn get(&self, param: Param) -> Result<f64> {
        let id = self.indices.id_of_param(param);
        call_with_error(|err, msg, len| unsafe {
            crate::ffi::AbstractState_keyed_output(self.handle, id, err, msg, len)
        })
    }

    /// Update the state using molar density and temperature.
    ///
    /// Shorthand for `update(InputPair::DmolarT, dmolar, t)`.
    #[inline]
    pub fn update_dmolar_t(&mut self, dmolar: f64, t: f64) -> Result<()> {
        self.update(InputPair::DmolarT, dmolar, t)
    }

    /// Current pressure in pascals.
    ///
    /// Equivalent to `get(Param::P)`.
    #[inline]
    pub fn pressure(&self) -> Result<f64> {
        self.get(Param::P)
    }

    /// Impose a phase classification prior to the next state update.
    ///
    /// Some iterative schemes benefit from constraining CoolProp to a specific phase branch.
    /// Pass [`Phase::NotImposed`] (via [`unspecify_phase`](Self::unspecify_phase)) to release the
    /// constraint.
    pub fn specify_phase(&mut self, phase: Phase) -> Result<()> {
        let token = phase.specifier_token();
        let phase = CString::new(token).map_err(|source| Error::EmbeddedNul {
            label: "phase specifier",
            source,
        })?;
        call_with_error(|err, msg, len| unsafe {
            crate::ffi::AbstractState_specify_phase(self.handle, phase.as_ptr(), err, msg, len);
        })
    }

    /// Remove any previously imposed phase constraint.
    pub fn unspecify_phase(&mut self) -> Result<()> {
        call_with_error(|err, msg, len| unsafe {
            crate::ffi::AbstractState_unspecify_phase(self.handle, err, msg, len);
        })
    }

    /// Comma-separated CoolProp fluid identifiers that are currently loaded.
    ///
    /// For pure fluids this matches the string passed to [`new`](Self::new); for mixtures, CoolProp
    /// returns the expanded component list.
    pub fn fluid_names(&self) -> Result<String> {
        let mut buffer = [0 as c_char; DEFAULT_STR_BUF_LEN];
        call_with_error(|err, msg, len| unsafe {
            crate::ffi::AbstractState_fluid_names(self.handle, buffer.as_mut_ptr(), err, msg, len);
        })?;
        Ok(crate::c_buf_to_string(&buffer))
    }

    /// Name of the active CoolProp backend (e.g., `"HEOS"`, `"REFPROP"`).
    pub fn backend_name(&self) -> Result<String> {
        let mut buffer = [0 as c_char; DEFAULT_STR_BUF_LEN];
        call_with_error(|err, msg, len| unsafe {
            crate::ffi::AbstractState_backend_name(self.handle, buffer.as_mut_ptr(), err, msg, len);
        })?;
        Ok(crate::c_buf_to_string(&buffer))
    }

    /// Query a string-valued fluid parameter.
    ///
    /// `param` uses the CoolProp keyword (such as `"aliases"` or `"CAS"`). The returned string is
    /// owned and resized internally to ensure the full result is captured.
    ///
    /// # Errors
    ///
    /// Returns an error if the parameter name contains a NUL byte or CoolProp fails to populate the
    /// field.
    pub fn fluid_param_string(&self, param: &str) -> Result<String> {
        let param = CString::new(param).map_err(|source| Error::EmbeddedNul {
            label: "param",
            source,
        })?;
        let mut capacity = DEFAULT_STR_BUF_LEN;
        loop {
            let mut buffer = vec![0 as c_char; capacity];
            match call_with_error(|err, msg, buflen| unsafe {
                crate::ffi::AbstractState_fluid_param_string(
                    self.handle,
                    param.as_ptr(),
                    buffer.as_mut_ptr(),
                    capacity as c_long,
                    err,
                    msg,
                    buflen,
                );
            }) {
                Ok(()) if !buffer_saturated(&buffer) => {
                    return Ok(crate::c_buf_to_string(&buffer));
                }
                Ok(()) => {
                    capacity *= 2;
                }
                Err(err) => return Err(err),
            }
        }
    }

    /// Determine the current thermodynamic phase classification.
    ///
    /// Wraps `AbstractState::phase` from CoolProp and maps the integer code into the
    /// [`Phase`](crate::Phase) enum.
    pub fn phase(&self) -> Result<Phase> {
        let code = call_with_error(|err, msg, len| unsafe {
            crate::ffi::AbstractState_phase(self.handle, err, msg, len)
        })?;
        Phase::from_code(code).ok_or(Error::UnknownPhaseCode(code as i64))
    }

    /// Property evaluation at the saturated liquid state associated with the current conditions.
    pub fn saturated_liquid_keyed_output(&self, param: Param) -> Result<f64> {
        let id = self.indices.id_of_param(param);
        call_with_error(|err, msg, len| unsafe {
            crate::ffi::AbstractState_saturated_liquid_keyed_output(self.handle, id, err, msg, len)
        })
    }

    /// Property evaluation at the saturated vapor state associated with the current conditions.
    pub fn saturated_vapor_keyed_output(&self, param: Param) -> Result<f64> {
        let id = self.indices.id_of_param(param);
        call_with_error(|err, msg, len| unsafe {
            crate::ffi::AbstractState_saturated_vapor_keyed_output(self.handle, id, err, msg, len)
        })
    }

    /// Property evaluation for an explicit saturation phase (`liquid`, `gas`, or `twophase`).
    ///
    /// Fails if the supplied `phase` lacks a saturation token (e.g., supercritical states).
    pub fn keyed_output_sat_state(&self, phase: Phase, param: Param) -> Result<f64> {
        let token = phase.saturation_token().ok_or_else(|| {
            Error::InvalidInput(format!(
                "phase {phase:?} cannot be used for saturation outputs"
            ))
        })?;
        let phase = CString::new(token).map_err(|source| Error::EmbeddedNul {
            label: "phase",
            source,
        })?;
        let id = self.indices.id_of_param(param);
        call_with_error(|err, msg, len| unsafe {
            crate::ffi::AbstractState_keyed_output_satState(
                self.handle,
                phase.as_ptr(),
                id,
                err,
                msg,
                len,
            )
        })
    }

    /// First derivative along the saturation curve (`d of / d wrt`).
    pub fn first_saturation_deriv(&self, of: Param, wrt: Param) -> Result<f64> {
        let of = self.indices.id_of_param(of);
        let wrt = self.indices.id_of_param(wrt);
        call_with_error(|err, msg, len| unsafe {
            crate::ffi::AbstractState_first_saturation_deriv(self.handle, of, wrt, err, msg, len)
        })
    }

    /// First partial derivative of one property with respect to another at constant third property.
    pub fn first_partial_deriv(&self, of: Param, wrt: Param, constant: Param) -> Result<f64> {
        let of = self.indices.id_of_param(of);
        let wrt = self.indices.id_of_param(wrt);
        let constant = self.indices.id_of_param(constant);
        call_with_error(|err, msg, len| unsafe {
            crate::ffi::AbstractState_first_partial_deriv(
                self.handle,
                of,
                wrt,
                constant,
                err,
                msg,
                len,
            )
        })
    }

    /// Second derivative along the saturation surface with mixed dependence.
    pub fn second_two_phase_deriv(
        &self,
        of1: Param,
        wrt1: Param,
        constant1: Param,
        wrt2: Param,
        constant2: Param,
    ) -> Result<f64> {
        let of1 = self.indices.id_of_param(of1);
        let wrt1 = self.indices.id_of_param(wrt1);
        let constant1 = self.indices.id_of_param(constant1);
        let wrt2 = self.indices.id_of_param(wrt2);
        let constant2 = self.indices.id_of_param(constant2);
        call_with_error(|err, msg, len| unsafe {
            crate::ffi::AbstractState_second_two_phase_deriv(
                self.handle,
                of1,
                wrt1,
                constant1,
                wrt2,
                constant2,
                err,
                msg,
                len,
            )
        })
    }

    /// General second-order partial derivative at fixed pairs of properties.
    pub fn second_partial_deriv(
        &self,
        of1: Param,
        wrt1: Param,
        constant1: Param,
        wrt2: Param,
        constant2: Param,
    ) -> Result<f64> {
        let of1 = self.indices.id_of_param(of1);
        let wrt1 = self.indices.id_of_param(wrt1);
        let constant1 = self.indices.id_of_param(constant1);
        let wrt2 = self.indices.id_of_param(wrt2);
        let constant2 = self.indices.id_of_param(constant2);
        call_with_error(|err, msg, len| unsafe {
            crate::ffi::AbstractState_second_partial_deriv(
                self.handle,
                of1,
                wrt1,
                constant1,
                wrt2,
                constant2,
                err,
                msg,
                len,
            )
        })
    }

    /// First two-phase derivative using CoolProp's spline interpolation scheme.
    pub fn first_two_phase_deriv_splined(
        &self,
        of: Param,
        wrt: Param,
        constant: Param,
        x_end: f64,
    ) -> Result<f64> {
        let of = self.indices.id_of_param(of);
        let wrt = self.indices.id_of_param(wrt);
        let constant = self.indices.id_of_param(constant);
        call_with_error(|err, msg, len| unsafe {
            crate::ffi::AbstractState_first_two_phase_deriv_splined(
                self.handle,
                of,
                wrt,
                constant,
                x_end,
                err,
                msg,
                len,
            )
        })
    }

    /// First derivative inside the two-phase region with analytical CoolProp routines.
    pub fn first_two_phase_deriv(&self, of: Param, wrt: Param, constant: Param) -> Result<f64> {
        let of = self.indices.id_of_param(of);
        let wrt = self.indices.id_of_param(wrt);
        let constant = self.indices.id_of_param(constant);
        call_with_error(|err, msg, len| unsafe {
            crate::ffi::AbstractState_first_two_phase_deriv(
                self.handle,
                of,
                wrt,
                constant,
                err,
                msg,
                len,
            )
        })
    }

    /// Set molar composition fractions for mixtures.
    ///
    /// `fractions` must sum to one; CoolProp enforces additional backend-specific constraints.
    pub fn set_fractions(&mut self, fractions: &[f64]) -> Result<()> {
        let len = fractions.len() as c_long;
        call_with_error(|err, msg, buflen| unsafe {
            crate::ffi::AbstractState_set_fractions(
                self.handle,
                fractions.as_ptr(),
                len,
                err,
                msg,
                buflen,
            );
        })
    }

    /// Set mass composition fractions for mixtures.
    ///
    /// `fractions` must sum to one; interpretation is backend dependent.
    pub fn set_mass_fractions(&mut self, fractions: &[f64]) -> Result<()> {
        #[cfg(coolprop_has_abstractstate_set_mass_fractions)]
        {
            let len = fractions.len() as c_long;
            call_with_error(|err, msg, buflen| unsafe {
                crate::ffi::AbstractState_set_mass_fractions(
                    self.handle,
                    fractions.as_ptr(),
                    len,
                    err,
                    msg,
                    buflen,
                );
            })
        }
        #[cfg(not(coolprop_has_abstractstate_set_mass_fractions))]
        {
            let _ = fractions;
            Err(Error::InvalidInput(
                "this CoolProp build does not expose AbstractState_set_mass_fractions".into(),
            ))
        }
    }

    fn estimated_component_capacity(&self) -> Result<usize> {
        let names = self.fluid_names()?;
        let count = names
            .split('&')
            .filter(|segment| !segment.trim().is_empty())
            .count();
        Ok(count.max(1))
    }

    /// Retrieve the current molar composition as a vector with automatic sizing.
    pub fn mole_fractions(&self) -> Result<Vec<f64>> {
        let mut capacity = self.estimated_component_capacity()?;
        loop {
            let mut fractions = vec![0.0; capacity];
            let mut count: c_long = 0;
            match call_with_error(|err, msg, buflen| unsafe {
                crate::ffi::AbstractState_get_mole_fractions(
                    self.handle,
                    fractions.as_mut_ptr(),
                    capacity as c_long,
                    &mut count,
                    err,
                    msg,
                    buflen,
                );
            }) {
                Ok(()) => {
                    let actual = count.max(0) as usize;
                    if actual > capacity {
                        capacity = actual.max(capacity * 2);
                        continue;
                    }
                    fractions.truncate(actual);
                    return Ok(fractions);
                }
                Err(err) => {
                    let msg = err.to_string();
                    if msg.contains("buffer") || msg.contains("Length of array") {
                        capacity = capacity.max(1) * 2;
                        continue;
                    }
                    return Err(err);
                }
            }
        }
    }

    /// Retrieve the current mass composition as a vector with automatic sizing.
    pub fn mass_fractions(&self) -> Result<Vec<f64>> {
        #[cfg(coolprop_has_abstractstate_get_mass_fractions)]
        {
            let mut capacity = self.estimated_component_capacity()?;
            loop {
                let mut fractions = vec![0.0; capacity];
                let mut count: c_long = 0;
                match call_with_error(|err, msg, buflen| unsafe {
                    crate::ffi::AbstractState_get_mass_fractions(
                        self.handle,
                        fractions.as_mut_ptr(),
                        capacity as c_long,
                        &mut count,
                        err,
                        msg,
                        buflen,
                    );
                }) {
                    Ok(()) => {
                        let actual = count.max(0) as usize;
                        if actual > capacity {
                            capacity = actual.max(capacity * 2);
                            continue;
                        }
                        fractions.truncate(actual);
                        return Ok(fractions);
                    }
                    Err(err) => {
                        let msg = err.to_string();
                        if msg.contains("buffer") || msg.contains("Length of array") {
                            capacity = capacity.max(1) * 2;
                            continue;
                        }
                        return Err(err);
                    }
                }
            }
        }
        #[cfg(not(coolprop_has_abstractstate_get_mass_fractions))]
        {
            Err(Error::InvalidInput(
                "this CoolProp build does not expose AbstractState_get_mass_fractions".into(),
            ))
        }
    }

    /// Retrieve saturation compositions for the specified phase (`liquid` or `gas`).
    pub fn mole_fractions_sat_state(&self, phase: Phase) -> Result<Vec<f64>> {
        let token = phase.saturation_token().ok_or_else(|| {
            Error::InvalidInput(format!(
                "phase {phase:?} cannot be used for saturation fractions"
            ))
        })?;
        let phase = CString::new(token).map_err(|source| Error::EmbeddedNul {
            label: "phase",
            source,
        })?;
        let mut capacity = self.estimated_component_capacity()?;
        loop {
            let mut fractions = vec![0.0; capacity];
            let mut count: c_long = 0;
            match call_with_error(|err, msg, buflen| unsafe {
                crate::ffi::AbstractState_get_mole_fractions_satState(
                    self.handle,
                    phase.as_ptr(),
                    fractions.as_mut_ptr(),
                    capacity as c_long,
                    &mut count,
                    err,
                    msg,
                    buflen,
                );
            }) {
                Ok(()) => {
                    let actual = count.max(0) as usize;
                    if actual > capacity {
                        capacity = actual.max(capacity * 2);
                        continue;
                    }
                    fractions.truncate(actual);
                    return Ok(fractions);
                }
                Err(err) => {
                    let msg = err.to_string();
                    if msg.contains("buffer") || msg.contains("Length of array") {
                        capacity = capacity.max(1) * 2;
                        continue;
                    }
                    return Err(err);
                }
            }
        }
    }

    /// Component fugacity in pascals.
    pub fn get_fugacity(&self, i: c_long) -> Result<f64> {
        call_with_error(|err, msg, len| unsafe {
            crate::ffi::AbstractState_get_fugacity(self.handle, i, err, msg, len)
        })
    }

    /// Component fugacity coefficient (dimensionless).
    pub fn get_fugacity_coefficient(&self, i: c_long) -> Result<f64> {
        call_with_error(|err, msg, len| unsafe {
            crate::ffi::AbstractState_get_fugacity_coefficient(self.handle, i, err, msg, len)
        })
    }

    /// Batched update using an input pair and simultaneous extraction of common outputs.
    ///
    /// Returns temperature, pressure, molar density, molar enthalpy, and molar entropy arrays in
    /// a single struct. The returned vectors always match the length of the input slices.
    pub fn update_and_common_out(
        &mut self,
        pair: InputPair,
        value1: &[f64],
        value2: &[f64],
    ) -> Result<BatchCommonOutputs> {
        if value1.len() != value2.len() {
            return Err(Error::InvalidInput(
                "value arrays must be the same length".into(),
            ));
        }
        let len = value1.len();
        let mut temperature = vec![0.0; len];
        let mut pressure = vec![0.0; len];
        let mut rhomolar = vec![0.0; len];
        let mut hmolar = vec![0.0; len];
        let mut smolar = vec![0.0; len];
        let id = self.indices.id_of_pair(pair);
        call_with_error(|err, msg, buflen| unsafe {
            crate::ffi::AbstractState_update_and_common_out(
                self.handle,
                id,
                value1.as_ptr(),
                value2.as_ptr(),
                len as c_long,
                temperature.as_mut_ptr(),
                pressure.as_mut_ptr(),
                rhomolar.as_mut_ptr(),
                hmolar.as_mut_ptr(),
                smolar.as_mut_ptr(),
                err,
                msg,
                buflen,
            );
        })?;
        Ok(BatchCommonOutputs {
            temperature,
            pressure,
            rhomolar,
            hmolar,
            smolar,
        })
    }

    /// Batched update returning a single additional property as an owned vector.
    pub fn update_and_1_out(
        &mut self,
        pair: InputPair,
        value1: &[f64],
        value2: &[f64],
        output: Param,
    ) -> Result<Vec<f64>> {
        if value1.len() != value2.len() {
            return Err(Error::InvalidInput(
                "value arrays must be the same length".into(),
            ));
        }
        let len = value1.len();
        let mut out = vec![0.0; len];
        let id = self.indices.id_of_pair(pair);
        let out_param = self.indices.id_of_param(output);
        call_with_error(|err, msg, buflen| unsafe {
            crate::ffi::AbstractState_update_and_1_out(
                self.handle,
                id,
                value1.as_ptr(),
                value2.as_ptr(),
                len as c_long,
                out_param,
                out.as_mut_ptr(),
                err,
                msg,
                buflen,
            );
        })?;
        Ok(out)
    }

    /// Batched update returning five arbitrary properties as owned vectors.
    pub fn update_and_5_out(
        &mut self,
        pair: InputPair,
        value1: &[f64],
        value2: &[f64],
        outputs: [Param; 5],
    ) -> Result<[Vec<f64>; 5]> {
        if value1.len() != value2.len() {
            return Err(Error::InvalidInput(
                "value arrays must be the same length".into(),
            ));
        }
        let len = value1.len();
        let mut out1 = vec![0.0; len];
        let mut out2 = vec![0.0; len];
        let mut out3 = vec![0.0; len];
        let mut out4 = vec![0.0; len];
        let mut out5 = vec![0.0; len];
        let id = self.indices.id_of_pair(pair);
        let mut outs = outputs.map(|p| self.indices.id_of_param(p));
        call_with_error(|err, msg, buflen| unsafe {
            crate::ffi::AbstractState_update_and_5_out(
                self.handle,
                id,
                value1.as_ptr(),
                value2.as_ptr(),
                len as c_long,
                outs.as_mut_ptr(),
                out1.as_mut_ptr(),
                out2.as_mut_ptr(),
                out3.as_mut_ptr(),
                out4.as_mut_ptr(),
                out5.as_mut_ptr(),
                err,
                msg,
                buflen,
            );
        })?;
        Ok([out1, out2, out3, out4, out5])
    }

    /// Override binary interaction parameters for mixture models.
    ///
    /// Arguments `i` and `j` index the components, `parameter` is the CoolProp keyword, and
    /// `value` is supplied in backend-specific units.
    pub fn set_binary_interaction_double(
        &mut self,
        i: c_long,
        j: c_long,
        parameter: &str,
        value: f64,
    ) -> Result<()> {
        let parameter = CString::new(parameter).map_err(|source| Error::EmbeddedNul {
            label: "parameter",
            source,
        })?;
        call_with_error(|err, msg, len| unsafe {
            crate::ffi::AbstractState_set_binary_interaction_double(
                self.handle,
                i,
                j,
                parameter.as_ptr(),
                value,
                err,
                msg,
                len,
            );
        })
    }

    /// Set custom coefficients for cubic equation-of-state alpha functions.
    pub fn set_cubic_alpha_c(
        &mut self,
        i: c_long,
        parameter: &str,
        c1: f64,
        c2: f64,
        c3: f64,
    ) -> Result<()> {
        let parameter = CString::new(parameter).map_err(|source| Error::EmbeddedNul {
            label: "parameter",
            source,
        })?;
        call_with_error(|err, msg, len| unsafe {
            crate::ffi::AbstractState_set_cubic_alpha_C(
                self.handle,
                i,
                parameter.as_ptr(),
                c1,
                c2,
                c3,
                err,
                msg,
                len,
            );
        })
    }

    /// Override a scalar fluid parameter on a per-component basis.
    pub fn set_fluid_parameter_double(
        &mut self,
        i: c_long,
        parameter: &str,
        value: f64,
    ) -> Result<()> {
        let parameter = CString::new(parameter).map_err(|source| Error::EmbeddedNul {
            label: "parameter",
            source,
        })?;
        call_with_error(|err, msg, len| unsafe {
            crate::ffi::AbstractState_set_fluid_parameter_double(
                self.handle,
                i,
                parameter.as_ptr(),
                value,
                err,
                msg,
                len,
            );
        })
    }

    /// Trigger CoolProp's phase-envelope construction for the current mixture.
    ///
    /// `level` controls the resolution/detail as understood by CoolProp.
    pub fn build_phase_envelope(&mut self, level: &str) -> Result<()> {
        let level = CString::new(level).map_err(|source| Error::EmbeddedNul {
            label: "level",
            source,
        })?;
        call_with_error(|err, msg, len| unsafe {
            crate::ffi::AbstractState_build_phase_envelope(
                self.handle,
                level.as_ptr(),
                err,
                msg,
                len,
            );
        })
    }

    /// Retrieve the full phase envelope as owned vectors.
    pub fn phase_envelope(&self) -> Result<PhaseEnvelope> {
        let mut actual_length: c_long = 0;
        let mut actual_components: c_long = 0;

        // First call with zero-length buffers to query required sizes.
        match call_with_error(|err, msg, buflen| unsafe {
            crate::ffi::AbstractState_get_phase_envelope_data_checkedMemory(
                self.handle,
                0,
                0,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                &mut actual_length,
                &mut actual_components,
                err,
                msg,
                buflen,
            );
        }) {
            Ok(()) => {}
            Err(err) => {
                // Fall back to default guesses if CoolProp rejects the size query.
                let msg = err.to_string();
                if !(msg.contains("length") || msg.contains("buffer")) {
                    return Err(err);
                }
                actual_length = 256;
                actual_components = self.estimated_component_capacity()? as c_long;
            }
        }

        let mut points_guess = actual_length.max(0) as usize;
        let mut components_guess = actual_components.max(0) as usize;
        if points_guess == 0 && components_guess == 0 {
            return Ok(PhaseEnvelope {
                temperature: Vec::new(),
                pressure: Vec::new(),
                rhomolar_liq: Vec::new(),
                rhomolar_vap: Vec::new(),
                x: Vec::new(),
                y: Vec::new(),
            });
        }
        if points_guess == 0 {
            points_guess = 256;
        }
        if components_guess == 0 {
            components_guess = 1;
        }

        loop {
            let mut temperature = vec![0.0; points_guess];
            let mut pressure = vec![0.0; points_guess];
            let mut rhomolar_vap = vec![0.0; points_guess];
            let mut rhomolar_liq = vec![0.0; points_guess];
            let mut x = vec![0.0; points_guess * components_guess];
            let mut y = vec![0.0; points_guess * components_guess];

            let mut reported_length: c_long = 0;
            let mut reported_components: c_long = 0;

            match call_with_error(|err, msg, buflen| unsafe {
                crate::ffi::AbstractState_get_phase_envelope_data_checkedMemory(
                    self.handle,
                    points_guess as c_long,
                    components_guess as c_long,
                    temperature.as_mut_ptr(),
                    pressure.as_mut_ptr(),
                    rhomolar_vap.as_mut_ptr(),
                    rhomolar_liq.as_mut_ptr(),
                    x.as_mut_ptr(),
                    y.as_mut_ptr(),
                    &mut reported_length,
                    &mut reported_components,
                    err,
                    msg,
                    buflen,
                );
            }) {
                Ok(()) => {}
                Err(err) => {
                    let msg = err.to_string();
                    if msg.contains("buffer") || msg.contains("length") {
                        points_guess = points_guess.max(1) * 2;
                        components_guess = components_guess.max(1) * 2;
                        continue;
                    }
                    return Err(err);
                }
            }

            let actual_points = reported_length.max(0) as usize;
            let actual_components = reported_components.max(0) as usize;
            if actual_points > points_guess || actual_components > components_guess {
                points_guess = points_guess.max(actual_points).max(1) * 2;
                components_guess = components_guess.max(actual_components).max(1);
                continue;
            }

            temperature.truncate(actual_points);
            pressure.truncate(actual_points);
            rhomolar_vap.truncate(actual_points);
            rhomolar_liq.truncate(actual_points);

            let x_flat = if actual_components == 0 || actual_points == 0 {
                Vec::new()
            } else {
                x[..actual_points * actual_components].to_vec()
            };
            let y_flat = if actual_components == 0 || actual_points == 0 {
                Vec::new()
            } else {
                y[..actual_points * actual_components].to_vec()
            };

            let x_matrix = if actual_components == 0 || actual_points == 0 {
                Vec::new()
            } else {
                reshape_phase_compositions(&x_flat, actual_points, actual_components)
            };
            let y_matrix = if actual_components == 0 || actual_points == 0 {
                Vec::new()
            } else {
                reshape_phase_compositions(&y_flat, actual_points, actual_components)
            };

            return Ok(PhaseEnvelope {
                temperature,
                pressure,
                rhomolar_liq,
                rhomolar_vap,
                x: x_matrix,
                y: y_matrix,
            });
        }
    }

    /// Build the spinodal curve for the current mixture.
    pub fn build_spinodal(&mut self) -> Result<()> {
        call_with_error(|err, msg, len| unsafe {
            crate::ffi::AbstractState_build_spinodal(self.handle, err, msg, len);
        })
    }

    /// Retrieve spinodal data (reduced temperature, density, and leading eigenvalue).
    pub fn spinodal_data(&self) -> Result<SpinodalCurve> {
        let mut capacity = 256usize;
        loop {
            let mut tau = vec![f64::NAN; capacity];
            let mut delta = vec![f64::NAN; capacity];
            let mut m1 = vec![f64::NAN; capacity];

            call_with_error(|err, msg, buflen| unsafe {
                crate::ffi::AbstractState_get_spinodal_data(
                    self.handle,
                    capacity as c_long,
                    tau.as_mut_ptr(),
                    delta.as_mut_ptr(),
                    m1.as_mut_ptr(),
                    err,
                    msg,
                    buflen,
                );
            })?;

            let actual_len = detect_filled_prefix(&tau, &delta, &m1);
            if actual_len >= capacity && capacity < 8192 {
                capacity *= 2;
                continue;
            }
            tau.truncate(actual_len);
            delta.truncate(actual_len);
            m1.truncate(actual_len);
            return Ok(SpinodalCurve { tau, delta, m1 });
        }
    }

    /// Enumerate all detected critical points with stability indicators.
    pub fn critical_points(&self) -> Result<Vec<CriticalPoint>> {
        let mut capacity = 4usize;
        loop {
            let mut temperature = vec![f64::NAN; capacity];
            let mut pressure = vec![f64::NAN; capacity];
            let mut rhomolar = vec![f64::NAN; capacity];
            let mut stability = vec![-1 as c_long; capacity];

            call_with_error(|err, msg, buflen| unsafe {
                crate::ffi::AbstractState_all_critical_points(
                    self.handle,
                    capacity as c_long,
                    temperature.as_mut_ptr(),
                    pressure.as_mut_ptr(),
                    rhomolar.as_mut_ptr(),
                    stability.as_mut_ptr(),
                    err,
                    msg,
                    buflen,
                );
            })?;

            let mut count = 0usize;
            for idx in 0..capacity {
                let t = temperature[idx];
                let p = pressure[idx];
                let rho = rhomolar[idx];
                if t.is_finite() && p.is_finite() && rho.is_finite() && t > 0.0 && p > 0.0 {
                    count = idx + 1;
                }
            }
            if count >= capacity && capacity < 64 {
                capacity *= 2;
                continue;
            }
            let mut result = Vec::with_capacity(count);
            for idx in 0..count {
                result.push(CriticalPoint {
                    temperature: temperature[idx],
                    pressure: pressure[idx],
                    rhomolar: rhomolar[idx],
                    stable: stability[idx] != 0,
                });
            }
            return Ok(result);
        }
    }
}

impl Drop for AbstractState {
    /// Release the underlying CoolProp state handle.
    fn drop(&mut self) {
        let _ = call_with_error(|err, msg, len| unsafe {
            crate::ffi::AbstractState_free(self.handle, err, msg, len);
        });
    }
}

impl fmt::Debug for AbstractState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let backend = self
            .backend_name()
            .unwrap_or_else(|_| String::from("<unavailable>"));
        let fluids = self
            .fluid_names()
            .unwrap_or_else(|_| String::from("<unavailable>"));
        f.debug_struct("AbstractState")
            .field("handle", &self.handle)
            .field("backend", &backend)
            .field("fluids", &fluids)
            .finish()
    }
}

fn call_with_error<R>(f: impl FnOnce(*mut c_long, *mut c_char, c_long) -> R) -> Result<R> {
    let mut err: c_long = 0;
    let mut buf = [0 as c_char; ERR_BUF_LEN];
    let result = f(
        &mut err as *mut c_long,
        buf.as_mut_ptr(),
        ERR_BUF_LEN as c_long,
    );
    if err != 0 {
        // Protect against non-terminated writes from the C side.
        buf[ERR_BUF_LEN - 1] = 0;
        let message = crate::c_buf_to_string(&buf);
        return Err(Error::CoolProp {
            code: err as i64,
            message,
        });
    }
    Ok(result)
}

fn buffer_saturated(buf: &[c_char]) -> bool {
    match buf.iter().position(|&c| c == 0) {
        Some(pos) => pos + 1 >= buf.len(),
        None => true,
    }
}

fn reshape_phase_compositions(flat: &[f64], points: usize, components: usize) -> Vec<Vec<f64>> {
    if points == 0 || components == 0 {
        return Vec::new();
    }
    debug_assert!(flat.len() >= points * components);
    let mut result = vec![vec![0.0; points]; components];
    for point in 0..points {
        for comp in 0..components {
            result[comp][point] = flat[point * components + comp];
        }
    }
    result
}

fn detect_filled_prefix(a: &[f64], b: &[f64], c: &[f64]) -> usize {
    let len = a.len().min(b.len()).min(c.len());
    let mut last = 0usize;
    for idx in 0..len {
        if a[idx].is_finite() || b[idx].is_finite() || c[idx].is_finite() {
            last = idx + 1;
        }
    }
    last
}

#[cfg(test)]
mod internal_tests {
    use super::{buffer_saturated, detect_filled_prefix, reshape_phase_compositions};

    #[test]
    fn buffer_saturated_detection() {
        let mut buf = vec![0i8; 4];
        buf[0] = b'a' as i8;
        buf[1] = 0;
        assert!(!buffer_saturated(&buf));
        // No NUL in buffer is treated as saturated
        let no_nul = vec![b'a' as i8, b'b' as i8, b'c' as i8];
        assert!(buffer_saturated(&no_nul));
        // NUL at the end indicates saturation
        let end_nul = vec![b'x' as i8, b'y' as i8, 0];
        assert!(buffer_saturated(&end_nul));
    }

    #[test]
    fn reshape_phase_compositions_handles_layouts() {
        // Point-major (points x components)
        // Two points, three components; each row sums to 1
        let flat_point_major = vec![
            0.2, 0.3, 0.5, // point 0
            0.1, 0.6, 0.3, // point 1
        ];
        let reshaped = reshape_phase_compositions(&flat_point_major, 2, 3);
        assert_eq!(reshaped.len(), 3); // components
        assert_eq!(reshaped[0], vec![0.2, 0.1]);
        assert_eq!(reshaped[1], vec![0.3, 0.6]);
        assert_eq!(reshaped[2], vec![0.5, 0.3]);
    }

    #[test]
    fn detect_filled_prefix_counts_any_finite() {
        let a = [f64::NAN, 1.0, f64::NAN, f64::NAN];
        let b = [f64::NAN, f64::NAN, 2.0, f64::NAN];
        let c = [f64::NAN, f64::NAN, f64::NAN, 3.0];
        // Up to index 3 we have some finite entries; trailing none are absent
        assert_eq!(detect_filled_prefix(&a, &b, &c), 4);
        let a2 = [f64::NAN, f64::NAN];
        let b2 = [f64::NAN, f64::NAN];
        let c2 = [f64::NAN, f64::NAN];
        assert_eq!(detect_filled_prefix(&a2, &b2, &c2), 0);
    }
}
