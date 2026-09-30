//! Modelo de erro do katu (E01-T06).
//!
//! Taxonomia **estável** definida antes de se espalhar pelo código. Regras:
//!
//! - `Error` é rico em contexto e encadeável (`#[source]`); nunca `Box<dyn Error>` na API do núcleo.
//! - Todo erro de I/O carrega o `path`; todo "não encontrado" carrega `kind`/`id`.
//! - `ErrorKind` é o **contrato de máquina**: `as_str()` (envelope) e `exit_code()` (processo).

use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};

use crate::diag::{Level, events};
use katu_policy::{ControlId, Evidence, RuleId};

/// Categoria estável de erro (contrato de máquina).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ErrorKind {
    /// Recurso não encontrado.
    NotFound,
    /// Entrada inválida.
    InvalidInput,
    /// Conflito de estado.
    Conflict,
    /// Erro de I/O.
    Io,
    /// Tempo esgotado.
    Timeout,
    /// Configuração inválida.
    Config,
    /// Esquema/vocabulário desconhecido.
    Schema,
    /// Bloqueado por segurança.
    UnsafeBlocked,
    /// Serviço indisponível.
    Unavailable,
    /// Erro interno.
    Internal,
}

impl ErrorKind {
    /// Identificador estável (`snake_case`) para o envelope de máquina.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::InvalidInput => "invalid_input",
            Self::Conflict => "conflict",
            Self::Io => "io",
            Self::Timeout => "timeout",
            Self::Config => "config",
            Self::Schema => "schema",
            Self::UnsafeBlocked => "unsafe_blocked",
            Self::Unavailable => "unavailable",
            Self::Internal => "internal",
        }
    }

    /// Código de saída do processo (mapa estável).
    #[must_use]
    pub const fn exit_code(self) -> u8 {
        match self {
            Self::InvalidInput => 2,
            Self::Io => 3,
            Self::NotFound => 4,
            Self::Conflict => 5,
            Self::Timeout => 6,
            Self::Config => 7,
            Self::Schema => 8,
            Self::UnsafeBlocked => 9,
            Self::Unavailable => 10,
            Self::Internal => 70,
        }
    }
}

/// Erro do katu, com contexto e cadeia de causas.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Recurso não encontrado.
    #[error("não encontrado: {kind} `{id}`")]
    NotFound {
        /// Tipo do recurso (ex.: `ficheiro`).
        kind: &'static str,
        /// Identificador do recurso.
        id: String,
    },
    /// Entrada inválida.
    #[error("entrada inválida: {message}")]
    InvalidInput {
        /// Descrição do problema.
        message: String,
    },
    /// Conflito de estado.
    #[error("conflito: {message}")]
    Conflict {
        /// Descrição do conflito.
        message: String,
    },
    /// Erro de I/O, sempre com o caminho.
    #[error("erro de I/O em `{path}`")]
    Io {
        /// Caminho afetado.
        path: String,
        /// Causa subjacente.
        #[source]
        source: std::io::Error,
    },
    /// Tempo esgotado.
    #[error("timeout de {millis} ms em `{operation}`")]
    Timeout {
        /// Operação que excedeu o tempo.
        operation: &'static str,
        /// Limite em milissegundos.
        millis: u64,
    },
    /// Configuração inválida.
    #[error("configuração inválida: {message}")]
    Config {
        /// Descrição do problema.
        message: String,
    },
    /// Esquema/vocabulário desconhecido (fail-closed).
    #[error("esquema inválido: {message}")]
    Schema {
        /// Descrição do problema.
        message: String,
    },
    /// Bloqueado por segurança (contenção).
    #[error("bloqueado por segurança: {message}")]
    UnsafeBlocked {
        /// Motivo do bloqueio.
        message: String,
    },
    /// Serviço indisponível.
    #[error("indisponível: {service}")]
    Unavailable {
        /// Serviço em falta.
        service: String,
    },
    /// Erro interno (invariante violada).
    #[error("erro interno: {message}")]
    Internal {
        /// Descrição do problema.
        message: String,
    },
}

impl Error {
    /// Categoria de máquina deste erro.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::NotFound { .. } => ErrorKind::NotFound,
            Self::InvalidInput { .. } => ErrorKind::InvalidInput,
            Self::Conflict { .. } => ErrorKind::Conflict,
            Self::Io { .. } => ErrorKind::Io,
            Self::Timeout { .. } => ErrorKind::Timeout,
            Self::Config { .. } => ErrorKind::Config,
            Self::Schema { .. } => ErrorKind::Schema,
            Self::UnsafeBlocked { .. } => ErrorKind::UnsafeBlocked,
            Self::Unavailable { .. } => ErrorKind::Unavailable,
            Self::Internal { .. } => ErrorKind::Internal,
        }
    }

    /// Erro de I/O com caminho.
    pub fn io(path: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    /// Entrada inválida.
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::InvalidInput {
            message: message.into(),
        }
    }

    /// Serviço indisponível (fail-closed, E03-T07).
    pub fn unavailable(service: impl Into<String>) -> Self {
        Self::Unavailable {
            service: service.into(),
        }
    }

    /// Erro interno.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }
}

/// Efeito de uma operação de tool, incluindo negação e indisponibilidade.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToolOutcome {
    /// Completo.
    Ok,
    /// Parcial (fez parte, falhou o resto).
    Partial,
    /// Negado por política/contenção, com a regra e a evidência (§29).
    Denied {
        /// Regra que negou.
        rule_id: RuleId,
        /// Evidência estruturada.
        evidence: Evidence,
    },
    /// Tempo esgotado.
    Timeout,
    /// Indisponível: falta um controlo (aprovação, memória, ...).
    Unavailable {
        /// Controlo em falta.
        control: ControlId,
        /// Regra que exige o controlo, quando aplicável (DF10).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rule_id: Option<RuleId>,
    },
}

impl ToolOutcome {
    /// `true` se a operação teve efeito aceitável.
    #[must_use]
    pub const fn is_success(&self) -> bool {
        matches!(self, Self::Ok | Self::Partial)
    }

    /// Regra que recusou, quando a recusa é acionável (DF10).
    ///
    /// É o que permite ao modelo **corrigir-se**: saber *qual* regra exige o quê, em vez de um
    /// controlo opaco.
    #[must_use]
    pub fn rule_id(&self) -> Option<&RuleId> {
        match self {
            Self::Denied { rule_id, .. } => Some(rule_id),
            Self::Unavailable { rule_id, .. } => rule_id.as_ref(),
            _ => None,
        }
    }
}

/// Erro associado a um resultado de tool, com proveniência (§38).
#[derive(Debug, thiserror::Error)]
#[error("{component}/{scope}: {message}")]
pub struct OutcomeError {
    /// Componente que falhou (ex.: `contain`).
    pub component: String,
    /// Categoria estável.
    pub category: ErrorKind,
    /// Âmbito (ex.: caminho, comando).
    pub scope: String,
    /// Mensagem legível.
    pub message: String,
}

/// Adquire um `Mutex`, recuperando de *poison* (regista aviso estruturado).
///
/// Um *panic* a segurar o lock não deve propagar; recuperamos o guard e deixamos rasto em `diag`.
pub fn lock_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poison) => {
            crate::event!(Level::Warn, events::LOCK_RECOVERED);
            PoisonError::into_inner(poison)
        }
    }
}

#[cfg(test)]
mod tests;
