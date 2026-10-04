//! Resolver do modelo de cada passo (`Q2/PI_GAINS`).
//!
//! Vive num módulo próprio para manter `mod.rs` sob o teto de linhas; é reexportado por `agent`.

use katu_core::provider::ModelSpec;
use katu_policy::Phase;

/// Resolve o modelo de cada passo do turno (`Q2/PI_GAINS`).
///
/// É o equivalente do `prepareRequest` do pi: o modelo pode mudar **entre passos** do mesmo
/// turno, não só entre turnos. O modelo explícito do utilizador nunca é sobreposto (E12-T10):
/// quando ele fixa um modelo, o kernel não fornece resolver.
pub(crate) trait StepModel: Send + Sync {
    /// Modelo para o passo, dada a fase corrente; `None` mantém o modelo fixo.
    fn model_for(&self, phase: Phase, step: u32) -> Option<ModelSpec>;
}
