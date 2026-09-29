//! Fallback quando a instrumentação está compilada fora (`feature = "instrument"` desligada).

use super::Level;

/// `true` se a instrumentação está ligada (sempre `false` sem a feature).
#[must_use]
pub const fn enabled() -> bool {
    false
}

/// No-op que marca `level`/`event` como usados (evita `unused_imports` no chamador).
pub const fn noop_event(_level: Level, _event: &'static str) {}
