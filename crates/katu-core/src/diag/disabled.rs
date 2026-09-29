//! Fallback quando a instrumentação está compilada fora (`feature = "instrument"` desligada).

/// `true` se a instrumentação está ligada (sempre `false` sem a feature).
#[must_use]
pub const fn enabled() -> bool {
    false
}
