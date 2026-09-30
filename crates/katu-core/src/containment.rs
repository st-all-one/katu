//! Contenção determinística (**soft**) e modos — E07-T01.
//!
//! No **MVP não há jail de SO**: o katu corre global de facto, como o utilizador que o evocou. O
//! que limita a IA são as **travas determinísticas** da política imposta na operação
//! (`Allow`/`Deny`/`RequireApproval`/`NeedsHuman`). Este módulo torna o modo **explícito e
//! honesto**, dá o gancho para a jail futura (E17) **sem a implementar** e nunca promete
//! isolamento de kernel.
//!
//! Regra de honestidade: `SandboxEnforcement` é **sempre** `Soft` no MVP; pedir `Full`/`Partial`
//! devolve [`ContainmentError::Unavailable`] (fail-closed), nunca execução livre.

use serde::{Deserialize, Serialize};

use crate::diag::{Level, events};
use katu_policy::{Capability, ResolvedPath};

/// Modo de contenção efetivo. No MVP só existe contenção **soft**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Containment {
    /// Travas determinísticas da política impostas na operação; **sem** isolamento de kernel.
    Soft,
    /// Global de facto, como o utilizador (sem travas de política). Não é o modo do MVP.
    Unconfined,
}

impl Containment {
    /// Modo por omissão do MVP.
    #[must_use]
    pub const fn mvp() -> Self {
        Self::Soft
    }

    /// Identificador estável (logs/UI).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Soft => "soft",
            Self::Unconfined => "unconfined",
        }
    }

    /// Nível de garantia relatado **honestamente** (sempre `Soft` no MVP).
    #[must_use]
    pub const fn enforcement(self) -> SandboxEnforcement {
        match self {
            Self::Soft | Self::Unconfined => SandboxEnforcement::Soft,
        }
    }

    /// `true` se a política é imposta na operação (só [`Containment::Soft`] no MVP).
    #[must_use]
    pub const fn enforces_policy(self) -> bool {
        matches!(self, Self::Soft)
    }
}

/// Nível de isolamento de kernel **efetivamente em vigor**. No MVP é sempre `Soft`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxEnforcement {
    /// Sem isolamento de kernel; apenas travas determinísticas.
    Soft,
    /// Isolamento completo (bwrap/Landlock/seccomp) — jail futura (E17).
    Full,
    /// Isolamento parcial — jail futura (E17).
    Partial,
}

impl SandboxEnforcement {
    /// Identificador estável (logs/UI).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Soft => "soft",
            Self::Full => "full",
            Self::Partial => "partial",
        }
    }

    /// `true` se há isolamento de kernel (nunca no MVP).
    #[must_use]
    pub const fn is_kernel_isolated(self) -> bool {
        matches!(self, Self::Full | Self::Partial)
    }
}

/// Estado de contenção para UI/logs (E07-T01): a limitação é **declarada**, nunca escondida.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainmentStatus {
    /// Modo de contenção.
    pub mode: Containment,
    /// Garantia efetiva (sempre `Soft` no MVP).
    pub enforcement: SandboxEnforcement,
}

impl ContainmentStatus {
    /// Estado do MVP: contenção soft, sem isolamento de kernel.
    #[must_use]
    pub const fn mvp() -> Self {
        Self {
            mode: Containment::mvp(),
            enforcement: SandboxEnforcement::Soft,
        }
    }

    /// `true` se há isolamento de kernel (nunca no MVP).
    #[must_use]
    pub const fn kernel_isolated(self) -> bool {
        self.enforcement.is_kernel_isolated()
    }
}

/// Capacidades por omissão do workspace (E07-T05): ler e escrever **só** sob a raiz.
///
/// São derivadas da raiz (não guardadas no log): a política destranca o que está sob a raiz e
/// exige autorização para o resto. Sem workspace definido, não há concessão — falha fechado.
#[must_use]
pub fn workspace_capabilities(root: &ResolvedPath) -> Vec<Capability> {
    let _span = crate::trace_fn!("containment::workspace_capabilities");

    vec![Capability::Workspace { root: root.clone() }]
}

/// Emite o modo de contenção no diagnóstico (E07-T01). Honestidade obrigatória: diz
/// `soft`/`kernel_isolated=false`; nunca promete isolamento de SO.
#[cfg_attr(
    not(feature = "instrument"),
    allow(
        unused_variables,
        reason = "o macro no-op ignora os campos (custo zero)"
    )
)]
pub fn announce(status: ContainmentStatus) {
    let _span = crate::trace_fn!("containment::announce");

    crate::event!(
        Level::Info,
        events::CONTAIN_MODE,
        "mode" => status.mode.as_str(),
        "enforcement" => status.enforcement.as_str(),
        "kernel_isolated" => status.kernel_isolated(),
    );
}

/// Gancho para a jail de SO futura (E17). No MVP **não** há implementação.
pub trait Jail {
    /// Tenta adquirir a jail no nível pedido.
    ///
    /// # Errors
    /// [`ContainmentError::Unavailable`] enquanto a jail não existe (fail-closed).
    fn acquire(&self, mode: SandboxEnforcement) -> Result<(), ContainmentError>;
}

/// Jail ausente (MVP): pedir `Full`/`Partial` **falha**; nunca executa livre.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoJail;

impl Jail for NoJail {
    fn acquire(&self, mode: SandboxEnforcement) -> Result<(), ContainmentError> {
        let _span = crate::fn_span!(
            Level::Trace,
            events::CONTAIN_CHECK,
            "containment::jail_acquire"
        );
        match mode {
            SandboxEnforcement::Soft => Ok(()),
            SandboxEnforcement::Full | SandboxEnforcement::Partial => {
                crate::event!(Level::Debug, events::CONTAIN_DENY, "mode" => mode.as_str());
                Err(ContainmentError::Unavailable { mode })
            }
        }
    }
}

/// Erro de contenção.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainmentError {
    /// A jail pedida não está implementada (E17); **fail-closed**.
    Unavailable {
        /// Nível pedido.
        mode: SandboxEnforcement,
    },
}

impl std::fmt::Display for ContainmentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _span = crate::trace_fn!("containment::fmt");

        match self {
            Self::Unavailable { mode } => {
                write!(formatter, "jail indisponível: {} (E17)", mode.as_str())
            }
        }
    }
}

impl std::error::Error for ContainmentError {}

#[cfg(test)]
mod tests;
