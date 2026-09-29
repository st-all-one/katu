//! Tipos de valor da porta `Memory` (DF6): enums fechados, sem tipos do knudge.

use serde::{Deserialize, Serialize};

/// Espécie da nota (enum **fechado**; espelha o contrato do knudge sem o importar).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum NoteType {
    /// Facto.
    Fact,
    /// Decisão.
    Decision,
    /// Questão.
    Question,
    /// Tarefa.
    Task,
    /// Definição.
    Def,
    /// Erro.
    Error,
    /// Excerto de código.
    Snippet,
    /// Ligação.
    Link,
    /// Meta.
    Meta,
    /// Risco.
    Risk,
}

/// Estado de vida de uma nota (enum **fechado**).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Status {
    /// Ativa.
    Active,
    /// Em curso.
    InProgress,
    /// Bloqueada.
    Blocked,
    /// Fechada.
    Closed,
    /// Substituída.
    Superseded,
    /// Esquecida (soft-delete).
    Forgotten,
}

/// Base de evidência de um score (DF5): medido ou inferido.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Basis {
    /// Medido (número com artefacto).
    Measured,
    /// Inferido (heurística).
    Inferred,
}

/// Referência opaca a uma nota (nunca um tipo do knudge).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NoteRef(String);

impl NoteRef {
    /// Constrói uma referência a partir do identificador.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Identificador da nota.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Âncora de código (ficheiro ou glob).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Anchor(String);

impl Anchor {
    /// Constrói uma âncora a partir do caminho/glob.
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    /// Caminho/glob.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Score de similaridade em **pontos base** (`0..=10_000`), determinístico (E18-T01): sem `f32`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Score(u16);

impl Score {
    /// Pontos base máximos (1.0).
    pub const MAX_BASIS_POINTS: u16 = 10_000;

    /// Constrói um score a partir de pontos base (`None` se `> 10_000`).
    #[must_use]
    pub const fn from_basis_points(points: u16) -> Option<Self> {
        if points <= Self::MAX_BASIS_POINTS {
            Some(Self(points))
        } else {
            None
        }
    }

    /// Pontos base (`0..=10_000`).
    #[must_use]
    pub const fn as_basis_points(self) -> u16 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::Score;

    #[test]
    fn score_rejects_out_of_range() {
        assert!(Score::from_basis_points(9_200).is_some());
        assert!(Score::from_basis_points(10_000).is_some());
        assert!(Score::from_basis_points(10_001).is_none());
    }

    #[test]
    fn score_orders_by_magnitude() {
        let low = Score::from_basis_points(7_500);
        let high = Score::from_basis_points(9_200);
        assert!(low < high);
    }
}
