//! Evidência tipada (DF5/E09-T05): **a evidência viaja com o número**.
//!
//! Um número que descreve desempenho carrega a sua **base** ([`EvidenceBasis`]) e, quando a base o
//! exige, uma referência ao **artefacto cru** que o produziu ([`ArtifactRef`]). Regras travadas:
//! a base não muda numa agregação; `unpriced` é zero, nunca adivinhado; um número sem artefacto
//! não fundamenta decisão.
//!
//! ```
//! use katu_core::evidence::{ArtifactRef, EvidenceBasis, Metric, Unit};
//! let metric = Metric::new(
//!     "startup_ms", 12.0, Unit::Millis, EvidenceBasis::Measured,
//!     Some(ArtifactRef::new("bench/startup/hyperfine.json")),
//! )?;
//! assert!(metric.is_publishable());
//! # Ok::<(), katu_core::evidence::EvidenceError>(())
//! ```

use serde::{Deserialize, Serialize};

/// Base de evidência de um número (DF5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceBasis {
    /// Medido diretamente (relógio, contador).
    Measured,
    /// Inferido por agregação/modelo.
    Inferred,
    /// Reportado pelo provider (ex.: `usage`).
    ProviderReported,
    /// Contrafactual contra um benchmark.
    BenchmarkCounterfactual,
    /// Observado numa amostra.
    Observed,
    /// Verificado por método imposto e nomeado.
    Verified,
    /// Preço ausente: zero, nunca adivinhado.
    Unpriced,
}

impl EvidenceBasis {
    /// `true` se a base exige um artefacto cru reproduzível.
    #[must_use]
    pub const fn requires_artifact(self) -> bool {
        matches!(
            self,
            Self::Measured
                | Self::ProviderReported
                | Self::BenchmarkCounterfactual
                | Self::Observed
                | Self::Verified
        )
    }

    /// `true` se o número pode fundamentar uma decisão (exclui `inferred`/`unpriced`).
    #[must_use]
    pub const fn is_publishable(self) -> bool {
        !matches!(self, Self::Inferred | Self::Unpriced)
    }

    /// Nome estável (mensagens de erro e serialização legível).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Measured => "measured",
            Self::Inferred => "inferred",
            Self::ProviderReported => "provider_reported",
            Self::BenchmarkCounterfactual => "benchmark_counterfactual",
            Self::Observed => "observed",
            Self::Verified => "verified",
            Self::Unpriced => "unpriced",
        }
    }
}

/// Unidade de uma métrica.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    /// Nanossegundos.
    Nanos,
    /// Milissegundos.
    Millis,
    /// Contagem inteira.
    Count,
    /// Fração `0..=1`.
    Ratio,
    /// Bytes.
    Bytes,
    /// Tokens.
    Tokens,
    /// Sem unidade declarada.
    Unspecified,
}

/// Referência a um artefacto cru (caminho relativo ao repositório).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRef {
    /// Caminho relativo ao repositório.
    pub path: String,
    /// O que o artefacto contém (opcional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl ArtifactRef {
    /// Cria uma referência a partir de um caminho.
    #[must_use]
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            note: None,
        }
    }

    /// Acrescenta uma nota descritiva.
    #[must_use]
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
}

/// Erros da base de evidência.
#[derive(Debug, thiserror::Error)]
pub enum EvidenceError {
    /// A base exige artefacto e nenhum foi dado.
    #[error("métrica `{name}` com base `{basis}` exige artefacto")]
    MissingArtifact {
        /// Nome da métrica.
        name: String,
        /// Base (nome estável).
        basis: &'static str,
    },
    /// `unpriced` só admite valor zero.
    #[error("métrica `{name}` `unpriced` tem valor diferente de zero")]
    UnpricedNonZero {
        /// Nome da métrica.
        name: String,
    },
    /// Agregação sem elementos.
    #[error("agregação sem métricas")]
    Empty,
    /// Agregação misturou bases.
    #[error("agregação mistura bases `{left}` e `{right}`")]
    MixedBasis {
        /// Base do primeiro elemento.
        left: &'static str,
        /// Base do elemento divergente.
        right: &'static str,
    },
}

/// Um número com a sua base e artefacto (DF5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Metric {
    /// Nome estável da métrica.
    pub name: String,
    /// Valor.
    pub value: f64,
    /// Unidade.
    pub unit: Unit,
    /// Base de evidência.
    pub basis: EvidenceBasis,
    /// Artefacto cru que produziu o número.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<ArtifactRef>,
}

impl Metric {
    /// Cria uma métrica validando a base (artefacto obrigatório quando a base o exige).
    ///
    /// # Erros
    /// Devolve [`EvidenceError::MissingArtifact`] se a base exigir artefacto e nenhum for dado, e
    /// [`EvidenceError::UnpricedNonZero`] se `unpriced` tiver valor diferente de zero.
    pub fn new(
        name: impl Into<String>,
        value: f64,
        unit: Unit,
        basis: EvidenceBasis,
        artifact: Option<ArtifactRef>,
    ) -> Result<Self, EvidenceError> {
        let name = name.into();
        if basis.requires_artifact() && artifact.is_none() {
            return Err(EvidenceError::MissingArtifact {
                name,
                basis: basis.as_str(),
            });
        }
        if basis == EvidenceBasis::Unpriced && value != 0.0 {
            return Err(EvidenceError::UnpricedNonZero { name });
        }
        Ok(Self {
            name,
            value,
            unit,
            basis,
            artifact,
        })
    }

    /// `true` se o número pode fundamentar decisão (base publicável **e** artefacto presente).
    #[must_use]
    pub fn is_publishable(&self) -> bool {
        self.basis.is_publishable() && self.artifact.is_some()
    }

    /// Soma métricas **da mesma base**, preservando base e artefacto (DF5).
    ///
    /// # Erros
    /// [`EvidenceError::Empty`] se a lista estiver vazia; [`EvidenceError::MixedBasis`] se as bases
    /// divergirem — a base **não** muda numa agregação.
    pub fn sum(
        name: impl Into<String>,
        metrics: &[Self],
        unit: Unit,
    ) -> Result<Self, EvidenceError> {
        let first = metrics.first().ok_or(EvidenceError::Empty)?;
        for metric in metrics {
            if metric.basis != first.basis {
                return Err(EvidenceError::MixedBasis {
                    left: first.basis.as_str(),
                    right: metric.basis.as_str(),
                });
            }
        }
        let value = metrics.iter().map(|metric| metric.value).sum();
        Self::new(name, value, unit, first.basis, first.artifact.clone())
    }
}

/// Converte um inteiro em `f64` para métricas.
#[allow(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    reason = "métrica: inteiro→f64; a precisão perdida fica muito abaixo da variância medida"
)]
#[must_use]
pub fn to_f64(value: u64) -> f64 {
    value as f64
}

#[cfg(test)]
mod tests;
