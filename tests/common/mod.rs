use std::sync::{Mutex, OnceLock};

pub fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[allow(dead_code)]
pub fn assert_close(actual: f64, expected: f64, rel_tol: f64, abs_tol: f64, context: &str) {
    let diff = (actual - expected).abs();
    let tol = abs_tol.max(expected.abs() * rel_tol);
    assert!(
        diff <= tol,
        "{context} mismatch: actual={actual}, expected={expected}, diff={diff}, tol={tol}"
    );
}
