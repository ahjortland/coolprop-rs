use std::{
    env, fs,
    path::{Path, PathBuf},
};

const HEADER_FILE: &str = "CoolPropLib.h";

struct BuildArtifacts {
    lib_path: PathBuf,
    lib_dir: PathBuf,
    include_dir: PathBuf,
    lib_name: String,
    link_static: bool,
}

fn main() {
    const WATCHED_ENVS: &[&str] = &[
        "COOLPROP_LIB_DIR",
        "COOLPROP_LIB_NAME",
        "COOLPROP_LINK_STATIC",
        "COOLPROP_LINK_CXX",
        "COOLPROP_CXX_STDLIB",
        "COOLPROP_LIB_PATH",
        "COOLPROP_INCLUDE_DIR",
        "COOLPROP_SOURCE_DIR",
        "COOLPROP_SHARED",
        "COOLPROP_BUILD_TYPE",
    ];

    for var in WATCHED_ENVS {
        println!("cargo:rerun-if-env-changed={var}");
    }

    let vendored = env::var_os("CARGO_FEATURE_VENDORED").is_some();
    let vendor_artifacts = vendored.then(build_vendored);

    if let Some(artifacts) = vendor_artifacts.as_ref() {
        println!(
            "cargo:rustc-env=COOLPROP_LIB_PATH={}",
            artifacts.lib_path.display()
        );
        println!(
            "cargo:rustc-env=COOLPROP_INCLUDE_DIR={}",
            artifacts.include_dir.display()
        );
        println!("cargo:rustc-env=COOLPROP_LIB_NAME={}", artifacts.lib_name);
        if artifacts.link_static {
            println!("cargo:rustc-env=COOLPROP_LINK_STATIC=1");
        }
    }

    let mut lib_path = env_var("COOLPROP_LIB_PATH");
    if let Some(artifacts) = vendor_artifacts.as_ref() {
        lib_path = Some(artifacts.lib_path.display().to_string());
    }

    let mut header_dir = env_var("COOLPROP_INCLUDE_DIR");
    if let Some(artifacts) = vendor_artifacts.as_ref() {
        header_dir = Some(artifacts.include_dir.display().to_string());
    }

    let lib_name = env_var("COOLPROP_LIB_NAME")
        .or_else(|| vendor_artifacts.as_ref().map(|a| a.lib_name.clone()))
        .unwrap_or_else(|| "CoolProp".to_string());

    let link_static = env_var("COOLPROP_LINK_STATIC")
        .as_ref()
        .map(|value| env_truthy_str(value))
        .or_else(|| vendor_artifacts.as_ref().map(|a| a.link_static))
        .unwrap_or(false);
    if link_static {
        println!("cargo:rustc-env=COOLPROP_LINK_STATIC=1");
    }

    let mut link_cxx = env_var("COOLPROP_LINK_CXX")
        .as_ref()
        .map(|value| env_truthy_str(value))
        .unwrap_or(false);
    if link_static && !link_cxx {
        link_cxx = true;
        println!("cargo:rustc-env=COOLPROP_LINK_CXX=1");
    }

    if let Some(ref path) = lib_path {
        println!("cargo:rustc-env=COOLPROP_LIB_PATH={path}");
    }

    if let Some(ref header_dir) = header_dir {
        println!("cargo:rustc-env=COOLPROP_INCLUDE_DIR={header_dir}");
    }

    println!("cargo:rustc-env=COOLPROP_LIB_NAME={lib_name}");

    let lib_dir = env_var("COOLPROP_LIB_DIR")
        .or_else(|| lib_path.as_ref().and_then(|path| parent_dir(path)))
        .or_else(|| vendor_artifacts.as_ref().map(|a| a.lib_dir.display().to_string()))
        .unwrap_or_else(|| {
            panic!(
                "COOLPROP_LIB_DIR is unset and no library path was provided; \
                 set COOLPROP_LIB_DIR or COOLPROP_LIB_PATH so rustc can locate CoolProp"
            )
        });

    emit_link_flags(&lib_dir, &lib_name, link_static, link_cxx);

    generate_bindings(header_dir);
}

fn build_vendored() -> BuildArtifacts {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_VENDORED");

    let src = env::var("COOLPROP_SOURCE_DIR")
        .map(PathBuf::from)
        .or_else(|_| {
            let here = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
            let vendored = here.join("vendor").join("CoolProp");
            if vendored.join("CMakeLists.txt").exists() {
                Ok(vendored)
            } else {
                Err(env::VarError::NotPresent)
            }
        })
        .expect("Set COOLPROP_SOURCE_DIR or add vendor/CoolProp submodule");

    let build_type = env::var("COOLPROP_BUILD_TYPE").unwrap_or_else(|_| "Release".into());
    let shared = env::var("COOLPROP_SHARED").unwrap_or_else(|_| "ON".into());
    let building_shared = shared.eq_ignore_ascii_case("ON");

    let mut cfg = cmake::Config::new(&src);
    cfg.profile(&build_type)
        .define("BUILD_SHARED_LIBS", &shared)
        .define("COOLPROP_SHARED_LIBRARY", &shared)
        .cflag("-DCOOLPROP_LIB")
        .cxxflag("-DCOOLPROP_LIB")
        .define("COOLPROP_LIB", "1")
        .define("BUILD_TESTING", "OFF")
        .define("COOLPROP_BUILD_TESTS", "OFF")
        .define(
            "COOLPROP_STATIC_LIBRARY",
            if shared == "ON" { "OFF" } else { "ON" },
        )
        .define("CMAKE_POSITION_INDEPENDENT_CODE", "ON")
        .no_build_target(true);

    let dst = cfg.build();
    println!("cargo:root={}", dst.display());

    let header_src = src.join("include").join(HEADER_FILE);
    let include_dir = dst.join("include");
    if !header_src.exists() {
        panic!(
            "{HEADER_FILE} not found at {}; ensure CoolProp sources are present",
            header_src.display()
        );
    }

    fs::create_dir_all(&include_dir)
        .expect("failed to create include directory in CoolProp build output");
    fs::copy(&header_src, include_dir.join(HEADER_FILE)).unwrap_or_else(|err| {
        panic!(
            "failed to copy {HEADER_FILE} from {}: {err}",
            header_src.display()
        )
    });

    let (lib_dir, lib_name, lib_path) = locate_coolprop_outputs(&dst, building_shared);

    BuildArtifacts {
        lib_path,
        lib_dir,
        include_dir,
        lib_name,
        link_static: !building_shared,
    }
}

fn emit_link_flags(lib_dir: &str, lib_name: &str, link_static: bool, link_cxx: bool) {
    println!("cargo:rustc-link-search=native={lib_dir}");

    if link_static {
        println!("cargo:rustc-link-lib=static={lib_name}");
    } else {
        println!("cargo:rustc-link-lib={lib_name}");
    }

    if link_cxx {
        if let Some(cxx_lib) = env_var("COOLPROP_CXX_STDLIB").or_else(default_cxx_stdlib) {
            println!("cargo:rustc-link-lib={cxx_lib}");
        }
    }
}

fn env_var(key: &str) -> Option<String> {
    env::var(key).ok().filter(|value| !value.is_empty())
}

fn parent_dir(path: &str) -> Option<String> {
    Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
}

fn default_cxx_stdlib() -> Option<String> {
    match env::var("CARGO_CFG_TARGET_OS").ok().as_deref() {
        Some("windows") => None,
        Some("macos") => Some("c++".to_string()),
        _ => Some("stdc++".to_string()),
    }
}

fn env_truthy_str(value: &str) -> bool {
    matches!(
        value,
        "1" | "true" | "TRUE" | "True" | "yes" | "YES" | "on" | "ON"
    )
}

fn locate_coolprop_outputs(dst: &Path, shared: bool) -> (PathBuf, String, PathBuf) {
    let base = "CoolProp";
    let (fname, alt_fname) = if cfg!(target_os = "windows") {
        if shared {
            (format!("{base}.dll"), format!("{base}.lib"))
        } else {
            (format!("{base}.lib"), format!("{base}.a"))
        }
    } else if cfg!(target_os = "macos") {
        if shared {
            (format!("lib{base}.dylib"), format!("lib{base}.a"))
        } else {
            (format!("lib{base}.a"), format!("lib{base}.dylib"))
        }
    } else if shared {
        (format!("lib{base}.so"), format!("lib{base}.a"))
    } else {
        (format!("lib{base}.a"), format!("lib{base}.so"))
    };

    let mut candidates: Vec<PathBuf> = vec![
        dst.join("lib").join(&fname),
        dst.join("lib").join(&alt_fname),
        dst.join(&fname),
        dst.join(&alt_fname),
        dst.join("build").join(&fname),
        dst.join("build").join(&alt_fname),
    ];

    for conf in ["Release", "RelWithDebInfo", "Debug"] {
        candidates.push(dst.join("lib").join(conf).join(&fname));
        candidates.push(dst.join("lib").join(conf).join(&alt_fname));
        candidates.push(dst.join(conf).join(&fname));
        candidates.push(dst.join(conf).join(&alt_fname));
    }

    let lib_path = candidates
        .into_iter()
        .find(|p| p.exists())
        .unwrap_or_else(|| {
            panic!(
                "CoolProp library not found under {} (looked for {})",
                dst.display(),
                fname
            )
        });
    let lib_dir = lib_path.parent().unwrap().to_path_buf();
    let cargo_link_name = "CoolProp".to_string();

    (lib_dir, cargo_link_name, lib_path)
}

fn generate_bindings(include_dir: Option<String>) {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    let mut candidates = Vec::new();

    if let Some(dir) = include_dir {
        candidates.push(PathBuf::from(dir).join(HEADER_FILE));
    }

    candidates.push(
        manifest_dir
            .join("vendor")
            .join("CoolProp")
            .join("include")
            .join(HEADER_FILE),
    );

    let header = candidates
        .into_iter()
        .find(|candidate| candidate.exists())
        .unwrap_or_else(|| {
            panic!(
                "Unable to locate {HEADER_FILE} for bindgen in provided or fallback include paths"
            );
        });

    println!("cargo:rerun-if-changed={}", header.display());

    let bindings = bindgen::Builder::default()
        .header(header.to_string_lossy())
        .allowlist_function("AbstractState_.*")
        .allowlist_function("PropsSI")
        .allowlist_function("HAPropsSI")
        .allowlist_function("get_input_pair_index")
        .allowlist_function("get_param_index")
        .allowlist_function("get_global_param_string")
        .allowlist_function("set_config_string")
        .allowlist_function("set_config_double")
        .allowlist_function("set_config_bool")
        .generate()
        .expect("bindgen generation failed");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("bindings.rs");
    bindings
        .write_to_file(out_path)
        .expect("failed to write bindgen output");
}
