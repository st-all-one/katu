//! Factos tipados que a política avalia (E02-T01). Nada de `String` crua para caminhos ou `argv`.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::paths::{ResolvedArgv, ResolvedPath};

/// Instante em milissegundos desde a época (relógio do kernel, injetado nos factos).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(u64);

impl Timestamp {
    /// Constrói a partir de milissegundos.
    #[must_use]
    pub const fn from_millis(millis: u64) -> Self {
        Self(millis)
    }

    /// Milissegundos desde a época.
    #[must_use]
    pub const fn as_millis(self) -> u64 {
        self.0
    }
}

/// Estado do caminho único (DF1), como valor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Phase {
    /// Tarefa aberta.
    Task,
    /// Conhecimento consultado.
    KnowledgeConsulted,
    /// Plano escrito.
    Planned,
    /// Implementação feita.
    Implemented,
    /// Verificação feita.
    Verified,
    /// Persistência feita.
    Persisted,
    /// Fechada.
    Closed,
}

/// Nome de tool (vocabulário fechado; novas tools = decisão de kernel).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToolName {
    /// Leitura.
    Read,
    /// Escrita.
    Write,
    /// Edição.
    Edit,
    /// Lixo recuperável.
    Trash,
    /// Execução de comando.
    Exec,
    /// Busca.
    Search,
    /// Memória.
    Memory,
    /// Planeamento.
    Plan,
    /// Compactação.
    Compact,
    /// Modelo (controlo do utilizador).
    Model,
    /// Thinking (controlo do utilizador).
    Thinking,
}

/// Argumentos tipados por tool (nunca `String` crua).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
#[non_exhaustive]
pub enum ToolArgs {
    /// Leitura de um caminho.
    Read {
        /// Caminho alvo.
        path: ResolvedPath,
    },
    /// Escrita de `bytes` num caminho.
    Write {
        /// Caminho alvo.
        path: ResolvedPath,
        /// Tamanho do conteúdo.
        bytes: u64,
    },
    /// Edição de um caminho.
    Edit {
        /// Caminho alvo.
        path: ResolvedPath,
    },
    /// Envio para o lixo.
    Trash {
        /// Caminho alvo.
        path: ResolvedPath,
    },
    /// Execução de um comando.
    Exec {
        /// Argumentos do comando.
        argv: ResolvedArgv,
        /// Diretório de trabalho.
        cwd: ResolvedPath,
    },
    /// Busca sob uma raiz.
    Search {
        /// Raiz da busca.
        root: ResolvedPath,
    },
    /// Operação de memória.
    Memory {
        /// Operação (dedup, anchor, outcome, …).
        op: String,
    },
    /// Planeamento.
    Plan,
    /// Outro (payload opaco).
    Other,
}

/// Facto de uso de ferramenta, já resolvido (DF2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolUse {
    /// Nome da tool.
    pub name: ToolName,
    /// Argumentos tipados.
    pub args: ToolArgs,
    /// Caminhos canonicalizados envolvidos.
    pub resolved_paths: Vec<ResolvedPath>,
    /// `argv` resolvido, quando aplicável.
    pub argv: Option<ResolvedArgv>,
    /// Diretório de trabalho.
    pub cwd: ResolvedPath,
}

/// Capacidade concedida no contexto corrente (DF2, DF4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
#[non_exhaustive]
pub enum Capability {
    /// Ler sob uma raiz.
    ReadPath {
        /// Raiz concedida.
        root: ResolvedPath,
    },
    /// Escrever sob uma raiz.
    WritePath {
        /// Raiz concedida.
        root: ResolvedPath,
    },
    /// Apagar sob uma raiz.
    DeletePath {
        /// Raiz concedida.
        root: ResolvedPath,
    },
    /// Executar um programa.
    Exec {
        /// Programa permitido.
        program: String,
    },
    /// Aceder a um host.
    Net {
        /// Host permitido.
        host: String,
    },
    /// Abrir PTY.
    SpawnPty,
    /// Sessão MCP.
    McpSession {
        /// Identificador da sessão.
        id: String,
    },
}

/// Consumo de orçamento dentro de uma tarefa.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetState {
    /// Escritas efetuadas.
    pub writes: u32,
    /// Bytes escritos.
    pub bytes: u64,
    /// Execuções efetuadas.
    pub execs: u32,
}

/// Conjunto de factos avaliados num veredicto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Facts {
    /// Instante atual (ms desde a época).
    pub now_millis: u64,
    /// Fase corrente do caminho único.
    pub phase: Phase,
    /// Uso de ferramenta a avaliar.
    pub tool: ToolUse,
    /// Capacidades concedidas.
    pub capabilities: Vec<Capability>,
    /// Consumo de orçamento.
    pub budget: BudgetState,
    /// Tools já concluídas na tarefa (para `RequireAfter`).
    pub completed: BTreeSet<ToolName>,
}
