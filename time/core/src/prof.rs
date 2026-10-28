//! Opt-in stage timing. Off unless `enable()` is called (CLI `--prof`) or the
//! file `%TEMP%\flowtime_prof.on` exists (for runs inside After Effects, where
//! the totals are appended to `%TEMP%\flowtime_prof.log`).

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::Instant;

// 0 = unknown, 1 = off, 2 = on
static STATE: AtomicU8 = AtomicU8::new(0);
static TOTALS: Mutex<BTreeMap<&'static str, (f64, u64)>> = Mutex::new(BTreeMap::new());

fn flag_file() -> std::path::PathBuf {
    std::env::temp_dir().join("flowtime_prof.on")
}

pub fn enable() {
    STATE.store(2, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    match STATE.load(Ordering::Relaxed) {
        2 => true,
        1 => false,
        _ => {
            let on = flag_file().exists();
            STATE.store(if on { 2 } else { 1 }, Ordering::Relaxed);
            on
        }
    }
}

pub struct Span {
    name: &'static str,
    start: Option<Instant>,
}

pub fn span(name: &'static str) -> Span {
    Span { name, start: if enabled() { Some(Instant::now()) } else { None } }
}

impl Drop for Span {
    fn drop(&mut self) {
        if let Some(s) = self.start {
            let ms = s.elapsed().as_secs_f64() * 1000.0;
            if let Ok(mut t) = TOTALS.lock() {
                let e = t.entry(self.name).or_insert((0.0, 0));
                e.0 += ms;
                e.1 += 1;
            }
        }
    }
}

/// Totals so far as text, one stage per line, then clears them.
pub fn report() -> String {
    let mut t = match TOTALS.lock() {
        Ok(t) => t,
        Err(_) => return String::new(),
    };
    let mut s = String::new();
    for (k, (ms, n)) in t.iter() {
        s += &format!("{k:<24} {ms:10.1} ms  x{n:<5} {:8.2} ms/call\n", ms / *n as f64);
    }
    t.clear();
    s
}

/// Appends the report to `%TEMP%\flowtime_prof.log` (used from the AE plugin).
pub fn dump_to_log(header: &str) {
    if !enabled() {
        return;
    }
    let body = report();
    let path = std::env::temp_dir().join("flowtime_prof.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        use std::io::Write;
        let _ = writeln!(f, "== {header}\n{body}");
    }
}
