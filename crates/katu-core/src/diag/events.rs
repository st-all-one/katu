//! Catálogo de identificadores de evento (DF9/E19).
//!
//! **Fonte única de verdade** dos nomes usados em `span!`/`event!`. Regras:
//!
//! - Formato `<subsistema>.<ação>` (≥ 2 segmentos), minúsculas `[a-z0-9_]`.
//! - Estáveis: mudar um id é uma decisão registada (quebra dashboards e séries históricas).
//! - Instrumentar **não** é observável pelo modelo nem pelo log de sessão.
//!
//! Camadas sem acesso a `katu-core` (ex.: `katu-policy`, por firewall) são instrumentadas **pelo
//! chamador** — o kernel abre o span em torno da avaliação.

/// Versão do catálogo. Incrementa ao remover/renomear ids.
pub const CATALOG_VERSION: u16 = 1;

macro_rules! catalog {
    ($(($name:ident, $id:literal, $doc:literal)),* $(,)?) => {
        $(
            #[doc = $doc]
            pub const $name: &str = $id;
        )*
        /// Todos os identificadores do catálogo (para validação e ferramentas).
        pub const ALL: &[&str] = &[$($id),*];
    };
}

catalog! {
    // -- Execução / arranque -------------------------------------------------
    (KATU_RUN, "katu.run", "Execução de topo do binário."),
    (KATU_SETUP, "katu.setup", "Configuração de arranque (diagnóstico, portas)."),
    (KATU_SHUTDOWN, "katu.shutdown", "Encerramento controlado."),

    // -- Sistema de ficheiros (borda) ----------------------------------------
    (FS_READ, "fs.read", "Leitura de um ficheiro."),
    (FS_WRITE, "fs.write", "Escrita atómica de um ficheiro."),
    (FS_RENAME, "fs.rename", "Movimento/renomeação atómica de um caminho."),
    (FS_LIST, "fs.list", "Listagem de um diretório (ordem canónica)."),
    (FS_STAT, "fs.stat", "Metadados de um ficheiro (mtime)."),

    // -- Kernel ------------------------------------------------------------------
    (KERNEL_STEP, "kernel.step", "Um passo da máquina de estados."),
    (KERNEL_TURN, "kernel.turn", "Um turno completo (entrada → paragem)."),
    (KERNEL_TRANSITION, "kernel.transition", "Transição de estado (guardas + efeitos)."),
    (KERNEL_REFUSAL, "kernel.refusal", "Recusa determinística (fail-closed)."),
    (KERNEL_STOP, "kernel.stop", "Critério de paragem atingido."),
    (KERNEL_BUDGET, "kernel.budget", "Verificação de um débito de orçamento."),
    (KERNEL_BUDGET_REFUSE, "kernel.budget_refuse", "Recusa por teto de orçamento."),
    (LOCK_RECOVERED, "lock.recovered", "Mutex recuperado de poison (um panic a segurar o lock)."),
    (LOG_APPEND, "log.append", "Anexação ao log (fonte da verdade)."),
    (LOG_REPLAY, "log.replay", "Reconstrução do estado a partir do log."),

    // -- Política (emitido pelo kernel) --------------------------------------
    (POLICY_EVALUATE, "policy.evaluate", "Avaliação das regras para uma ação."),
    (POLICY_ALLOW, "policy.allow", "Ação permitida."),
    (POLICY_DENY, "policy.deny", "Ação negada."),
    (POLICY_WAIVER, "policy.waiver", "Exceção explícita aplicada."),

    // -- Tools & contenção ---------------------------------------------------
    (TOOL_CALL, "tool.call", "Pedido de tool do modelo."),
    (TOOL_OK, "tool.ok", "Tool concluída com sucesso."),
    (TOOL_ERROR, "tool.error", "Tool falhou."),
    (TOOL_EXEC, "tool.exec", "Execução de comando (com contenção)."),
    (TOOL_READ, "tool.read", "Leitura via tool."),
    (TOOL_WRITE, "tool.write", "Escrita via tool."),
    (TOOL_EDIT, "tool.edit", "Edição via tool."),
    (TOOL_MOVE, "tool.move", "Movimento/renomeação via tool."),
    (TOOL_SEARCH, "tool.search", "Busca (ripgrep/índice)."),
    (TOOL_TRASH, "tool.trash", "Movimento para o lixo recuperável."),
    (CONTAIN_CHECK, "contain.check", "Verificação de contenção (traps suaves)."),
    (CONTAIN_DENY, "contain.deny", "Contenção negou a operação."),

    // -- Memória -------------------------------------------------------------
    (MEMORY_READ, "memory.read", "Leitura da porta de memória."),
    (MEMORY_WRITE, "memory.write", "Escrita pela porta de memória."),
    (MEMORY_RECALL, "memory.recall", "Recuperação (busca/grafo)."),
    (MEMORY_HANDOFF, "memory.handoff", "Handoff entre sessões."),
    (MEMORY_COMPACT, "memory.compact", "Compactação de memória."),
    (MEMORY_STATUS, "memory.status", "Estado da memória."),

    // -- Contexto / checkpoint -----------------------------------------------
    (CONTEXT_BUILD, "context.build", "Construção do contexto do modelo."),
    (CONTEXT_TRIM, "context.trim", "Recorte/seleção de contexto."),
    (CONTEXT_CHECKPOINT, "context.checkpoint", "Checkpoint de evidência."),

    // -- Persistência --------------------------------------------------------
    (STORE_LOAD, "store.load", "Carregamento do estado persistido."),
    (STORE_SAVE, "store.save", "Gravação do estado persistido."),

    // -- Providers -----------------------------------------------------------
    (PROVIDER_REQUEST, "provider.request", "Pedido ao endpoint de modelo."),
    (PROVIDER_TTFT, "provider.ttft", "Tempo até ao primeiro token."),
    (PROVIDER_CHUNK, "provider.chunk", "Fragmento recebido em streaming."),
    (PROVIDER_RETRY, "provider.retry", "Nova tentativa (backoff/hedging)."),
    (PROVIDER_ERROR, "provider.error", "Erro do provider."),

    // -- TUI -----------------------------------------------------------------
    (TUI_RENDER, "tui.render", "Desenho de um quadro."),
    (TUI_INPUT, "tui.input", "Entrada do utilizador."),
}

#[cfg(test)]
mod tests {
    use super::ALL;
    use std::collections::BTreeSet;

    #[test]
    fn catalog_is_well_formed() {
        for id in ALL {
            assert!(well_formed(id), "identificador inválido: {id}");
        }
    }

    #[test]
    fn catalog_has_no_duplicates() {
        let mut seen = BTreeSet::new();
        for id in ALL {
            assert!(seen.insert(*id), "identificador duplicado: {id}");
        }
    }

    /// `<subsistema>.<ação>`, ≥ 2 segmentos, cada `[a-z][a-z0-9_]{0,31}`.
    fn well_formed(id: &str) -> bool {
        let mut segments = id.split('.');
        let first = segments.next().unwrap_or("");
        if !segment_ok(first) {
            return false;
        }
        let mut count = 1_usize;
        for segment in segments {
            if !segment_ok(segment) {
                return false;
            }
            count = count.saturating_add(1);
        }
        count >= 2
    }

    fn segment_ok(segment: &str) -> bool {
        if segment.is_empty() || segment.len() > 32 {
            return false;
        }
        let mut chars = segment.chars();
        let first = chars.next().unwrap_or(' ');
        first.is_ascii_lowercase()
            && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    }
}
