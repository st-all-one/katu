//! Tipos de apresentação da conversa (E10): papel, entrada e estado da barra.
//!
//! Vivem fora de [`crate::App`] para manter o estado central sob o teto de tamanho de ficheiro.

/// Papel de uma entrada da conversa (cor e prefixo no render).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Mensagem do utilizador.
    User,
    /// Resposta do modelo.
    Assistant,
    /// Chamada/resultado de tool.
    Tool,
    /// Nota informativa.
    Info,
    /// Erro.
    Error,
}

/// Uma linha da conversa (durável na sessão; o `App` só a mostra).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Papel.
    pub role: Role,
    /// Texto.
    pub text: String,
}

/// Estado da barra de estado.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Status {
    /// Parado.
    #[default]
    Idle,
    /// À espera de um efeito (o utilizador não acha que travou).
    Working,
    /// Mensagem transitória.
    Message(String),
    /// Falha.
    Failure(String),
}
