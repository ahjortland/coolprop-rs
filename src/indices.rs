use std::{
    ffi::CString,
    os::raw::{c_char, c_int, c_long},
    sync::OnceLock,
};

use crate::Result;

/// Thermodynamic phase labels exposed by the CoolProp C API.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[allow(missing_docs)]
pub enum Phase {
    Liquid,
    Supercritical,
    SupercriticalGas,
    SupercriticalLiquid,
    CriticalPoint,
    Gas,
    TwoPhase,
    Unknown,
    NotImposed,
}

impl Phase {
    pub(crate) fn from_code(code: c_int) -> Option<Self> {
        match code {
            0 => Some(Self::Liquid),
            1 => Some(Self::Supercritical),
            2 => Some(Self::SupercriticalGas),
            3 => Some(Self::SupercriticalLiquid),
            4 => Some(Self::CriticalPoint),
            5 => Some(Self::Gas),
            6 => Some(Self::TwoPhase),
            7 => Some(Self::Unknown),
            8 => Some(Self::NotImposed),
            _ => None,
        }
    }

    pub(crate) fn specifier_token(self) -> &'static str {
        match self {
            Self::Liquid => "phase_liquid",
            Self::Supercritical => "phase_supercritical",
            Self::SupercriticalGas => "phase_supercritical_gas",
            Self::SupercriticalLiquid => "phase_supercritical_liquid",
            Self::CriticalPoint => "phase_critical_point",
            Self::Gas => "phase_gas",
            Self::TwoPhase => "phase_twophase",
            Self::Unknown => "phase_unknown",
            Self::NotImposed => "phase_not_imposed",
        }
    }

    pub(crate) fn saturation_token(self) -> Option<&'static str> {
        match self {
            Self::Liquid => Some("liquid"),
            Self::Gas => Some("gas"),
            Self::TwoPhase => Some("twophase"),
            _ => None,
        }
    }
}

macro_rules! coolprop_input_pairs {
    ($( $variant:ident => $name:literal ),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
        #[repr(usize)]
        #[allow(missing_docs)]
        pub enum InputPair {
            $( $variant, )+
        }

        impl InputPair {
            pub const ALL: &'static [InputPair] = &[
                $( InputPair::$variant, )+
            ];

            #[inline]
            pub fn as_coolprop_str(self) -> &'static str {
                match self {
                    $( InputPair::$variant => $name, )+
                }
            }
        }
    };
}

coolprop_input_pairs! {
    PT => "PT_INPUTS",
    QT => "QT_INPUTS",
    PQ => "PQ_INPUTS",
    QSmolar => "QSmolar_INPUTS",
    QSmass => "QSmass_INPUTS",
    HmolarQ => "HmolarQ_INPUTS",
    HmassQ => "HmassQ_INPUTS",
    DmolarQ => "DmolarQ_INPUTS",
    DmassQ => "DmassQ_INPUTS",
    HmolarP => "HmolarP_INPUTS",
    HmassP => "HmassP_INPUTS",
    PSmolar => "PSmolar_INPUTS",
    PSmass => "PSmass_INPUTS",
    PUmolar => "PUmolar_INPUTS",
    PUmass => "PUmass_INPUTS",
    HmolarSmolar => "HmolarSmolar_INPUTS",
    HmassSmass => "HmassSmass_INPUTS",
    SmolarT => "SmolarT_INPUTS",
    SmassT => "SmassT_INPUTS",
    DmolarT => "DmolarT_INPUTS",
    DmassT => "DmassT_INPUTS",
    DmolarP => "DmolarP_INPUTS",
    DmassP => "DmassP_INPUTS",
    DmolarHmolar => "DmolarHmolar_INPUTS",
    DmassHmass => "DmassHmass_INPUTS",
    DmolarSmolar => "DmolarSmolar_INPUTS",
    DmassSmass => "DmassSmass_INPUTS",
    DmolarUmolar => "DmolarUmolar_INPUTS",
    DmassUmass => "DmassUmass_INPUTS",
    HmolarT => "HmolarT_INPUTS",
    HmassT => "HmassT_INPUTS",
    TUmolar => "TUmolar_INPUTS",
    TUmass => "TUmass_INPUTS",
}

macro_rules! coolprop_params {
    ($( $variant:ident => $name:literal ),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
        #[repr(usize)]
        #[allow(missing_docs)]
        pub enum Param {
            $( $variant, )+
        }

        impl Param {
            pub const ALL: &'static [Param] = &[
                $( Param::$variant, )+
            ];

            #[inline]
            pub fn as_coolprop_str(self) -> &'static str {
                match self {
                    $( Param::$variant => $name, )+
                }
            }
        }
    };
}

coolprop_params! {
    T => "T",
    P => "P",
    Dmolar => "Dmolar",
    Hmolar => "Hmolar",
    Smolar => "Smolar",
    Umolar => "Umolar",
    Gmolar => "Gmolar",
    Helmholtzmolar => "Helmholtzmolar",
    Dmass => "Dmass",
    Hmass => "Hmass",
    Smass => "Smass",
    Umass => "Umass",
    Gmass => "Gmass",
    Helmholtzmass => "Helmholtzmass",
    Q => "Q",
    Delta => "Delta",
    Tau => "Tau",
    Cpmolar => "Cpmolar",
    Cpmass => "Cpmass",
    Cvmolar => "Cvmolar",
    Cvmass => "Cvmass",
    Cp0molar => "Cp0molar",
    Cp0mass => "Cp0mass",
    HmolarResidual => "Hmolar_residual",
    SmolarResidual => "Smolar_residual",
    GmolarResidual => "Gmolar_residual",
    HmolarIdealgas => "Hmolar_idealgas",
    SmolarIdealgas => "Smolar_idealgas",
    UmolarIdealgas => "Umolar_idealgas",
    HmassIdealgas => "Hmass_idealgas",
    SmassIdealgas => "Smass_idealgas",
    UmassIdealgas => "Umass_idealgas",
    Gwp20 => "GWP20",
    Gwp100 => "GWP100",
    Gwp500 => "GWP500",
    Fh => "FH",
    Hh => "HH",
    Ph => "PH",
    Odp => "ODP",
    Bvirial => "Bvirial",
    Cvirial => "Cvirial",
    DBvirialDt => "dBvirial_dT",
    DCvirialDt => "dCvirial_dT",
    GasConstant => "gas_constant",
    MolarMass => "molar_mass",
    Acentric => "acentric",
    DipoleMoment => "dipole_moment",
    RhomassReducing => "rhomass_reducing",
    RhomolarReducing => "rhomolar_reducing",
    RhomolarCritical => "rhomolar_critical",
    RhomassCritical => "rhomass_critical",
    TReducing => "T_reducing",
    TCritical => "T_critical",
    TTriple => "T_triple",
    TMax => "T_max",
    TMin => "T_min",
    PMin => "P_min",
    PMax => "P_max",
    PCritical => "p_critical",
    PReducing => "p_reducing",
    PTriple => "p_triple",
    FractionMin => "fraction_min",
    FractionMax => "fraction_max",
    TFreeze => "T_freeze",
    SpeedOfSound => "speed_of_sound",
    Viscosity => "viscosity",
    Conductivity => "conductivity",
    SurfaceTension => "surface_tension",
    Prandtl => "Prandtl",
    IsothermalCompressibility => "isothermal_compressibility",
    IsobaricExpansionCoefficient => "isobaric_expansion_coefficient",
    IsentropicExpansionCoefficient => "isentropic_expansion_coefficient",
    Z => "Z",
    FundamentalDerivativeOfGasDynamics => "fundamental_derivative_of_gas_dynamics",
    Pip => "PIP",
    Alphar => "alphar",
    DalpharDtauConstdelta => "dalphar_dtau_constdelta",
    DalpharDdeltaConsttau => "dalphar_ddelta_consttau",
    Alpha0 => "alpha0",
    Dalpha0DtauConstdelta => "dalpha0_dtau_constdelta",
    Dalpha0DdeltaConsttau => "dalpha0_ddelta_consttau",
    D2Alpha0Ddelta2Consttau => "d2alpha0_ddelta2_consttau",
    D3Alpha0Ddelta3Consttau => "d3alpha0_ddelta3_consttau",
    Phase => "Phase",
}

pub(crate) struct Indices {
    input_pair_ids: Box<[c_long]>,
    param_ids: Box<[c_long]>,
}

impl Indices {
    fn load() -> Self {
        unsafe fn query(f: unsafe extern "C" fn(*const c_char) -> c_long, name: &str) -> c_long {
            let s = CString::new(name).unwrap();
            unsafe { f(s.as_ptr()) }
        }

        unsafe {
            let input_pair_ids = {
                let mut ids = Vec::with_capacity(InputPair::ALL.len());
                for &pair in InputPair::ALL {
                    ids.push(query(
                        crate::ffi::get_input_pair_index,
                        pair.as_coolprop_str(),
                    ));
                }
                ids.into_boxed_slice()
            };

            let param_ids = {
                let mut params = Vec::with_capacity(Param::ALL.len());
                for &param in Param::ALL {
                    params.push(query(
                        crate::ffi::get_param_index,
                        param.as_coolprop_str(),
                    ));
                }
                params.into_boxed_slice()
            };

            Self {
                input_pair_ids,
                param_ids,
            }
        }
    }

    #[inline]
    pub fn id_of_pair(&self, ip: InputPair) -> c_long {
        self.input_pair_ids[ip as usize]
    }

    #[inline]
    pub fn id_of_param(&self, p: Param) -> c_long {
        self.param_ids[p as usize]
    }
}

static INDICES: OnceLock<Indices> = OnceLock::new();

pub(crate) fn global_indices() -> Result<&'static Indices> {
    if let Some(indices) = INDICES.get() {
        return Ok(indices);
    }
    let computed = Indices::load();
    match INDICES.set(computed) {
        Ok(_) => Ok(INDICES.get().expect("CoolProp indices initialized")),
        Err(_) => Ok(INDICES.get().expect("CoolProp indices initialized")),
    }
}
