//! Impressão determinística de estado/resultado (E19-T04).
//!
//! Mesma entrada → mesma impressão (hex de 64 bits), comparável entre execuções: uma mudança de
//! impressão sem mudança de código denuncia não-determinismo. O hash é FNV-1a e cada fragmento é
//! **prefixado com o comprimento**, pelo que a fronteira entre fragmentos não é ambígua.
//!
//! ```
//! let a = katu_core::fingerprint!("a", "bc");
//! let b = katu_core::fingerprint!("ab", "c");
//! assert_ne!(a, b);
//! assert_eq!(a, katu_core::fingerprint!("a", "bc"));
//! ```

/// Acumulador FNV-1a de 64 bits.
#[derive(Debug, Clone, Copy)]
pub struct Fingerprint(u64);

/// Primo FNV de 64 bits.
const PRIME: u64 = 0x0000_0100_0000_01b3;

/// Offset basis FNV-1a de 64 bits.
const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

impl Fingerprint {
    /// Novo acumulador (estado FNV-1a inicial).
    #[must_use]
    pub const fn new() -> Self {
        Self(OFFSET)
    }

    /// Acumula bytes crus.
    pub fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(PRIME);
        }
    }

    /// Acumula um fragmento **prefixado com o comprimento** (não ambíguo entre fronteiras).
    pub fn write_fragment<T: AsRef<[u8]>>(&mut self, part: T) {
        let bytes = part.as_ref();
        self.write(&u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
        self.write(bytes);
    }

    /// Valor do hash.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }

    /// Impressão em 16 hex.
    #[must_use]
    pub fn hex(self) -> String {
        format!("{:016x}", self.0)
    }
}

impl Default for Fingerprint {
    fn default() -> Self {
        Self::new()
    }
}

/// Hash de uma sequência de fragmentos (equivalente a [`Fingerprint::write_fragment`] em ordem).
#[must_use]
pub fn of(parts: &[&[u8]]) -> u64 {
    let mut fp = Fingerprint::new();
    for part in parts {
        fp.write_fragment(*part);
    }
    fp.value()
}

#[cfg(test)]
mod tests {
    use super::{Fingerprint, of};

    #[test]
    fn same_input_has_the_same_print() {
        assert_eq!(of(&[b"abc"]), of(&[b"abc"]));
        let mut fp = Fingerprint::new();
        fp.write_fragment("x");
        assert_eq!(fp.hex().len(), 16, "16 hex");
    }

    #[test]
    fn boundaries_are_unambiguous() {
        let first = of(&[b"a", b"bc"]);
        let second = of(&[b"ab", b"c"]);
        assert_ne!(first, second, "o comprimento prefixado separa fronteiras");
    }

    #[test]
    fn golden_print_is_stable() {
        // Golden: se o algoritmo mudar sem intenção, este teste falha (E19-T04).
        assert_eq!(of(&[b"katu"]), 0x14bd_a398_5c24_1c6a);
    }
}
