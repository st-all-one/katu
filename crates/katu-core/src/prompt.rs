//! Condensação determinística do contexto de projeto para o prompt (Q-19).
//!
//! O `AGENTS.md` é um **router**: bullets `- **Rótulo** (nota): [`alvo`](alvo).` onde o texto do
//! link repete o alvo. A sintaxe markdown custa tokens sem informação: medido em
//! `bench/e18/prompt/` (sonda Q-19, artefacto `raw.json`), a condensação corta **~29 %** dos bytes
//! do `AGENTS.md` (1591 → 1129 B) sem perder **nenhum** ponteiro.
//!
//! A transformação é **pura** e sem RNG: mesma entrada → mesmos bytes. Regras (todas sem perda):
//!
//! 1. `[texto](alvo)` com `texto` (sem backticks) igual ao `alvo` → só `texto`;
//! 2. removem-se os marcadores de negrito `**` (ênfase, não informação);
//! 3. colapsam-se linhas em branco consecutivas.
//!
//! **Sem cache.** A condensação é uma passagem linear sobre ~1,6 KB (a classe do `toon::colunar`:
//! 89 µs em dev, `bench/e18/atomics`), enquanto `fs.write` custa **25,8 ms** por escrita com
//! `sync_all` (`bench/e18/raw.json`). Um artefacto derivado em `.katu/` com validação por
//! *fingerprint* pagaria ~300× o custo do cálculo para introduzir risco de staleness. Rejeitado com
//! o número; o cálculo é feito em memória a cada arranque.

/// Condensa markdown sem perder informação (regras no cabeçalho do módulo).
///
/// Não é um compressor de prosa: só remove sintaxe redundante. Texto, alvos e estrutura ficam.
#[must_use]
pub fn condense(markdown: &str) -> String {
    let _span = crate::trace_fn!("prompt::condense");

    let mut out = String::with_capacity(markdown.len());
    let mut blank = false;
    for line in markdown.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            if !blank {
                out.push('\n');
            }
            blank = true;
            continue;
        }
        blank = false;
        out.push_str(&condense_line(line));
        out.push('\n');
    }
    out
}

/// Uma linha, sem a sintaxe redundante (link com texto igual ao alvo, negrito).
fn condense_line(line: &str) -> String {
    let _span = crate::trace_fn!("prompt::condense_line");

    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        let (before, from_open) = rest.split_at(open);
        out.push_str(before);
        match parse_link(from_open) {
            Some((text, target, consumed)) if text.trim_matches('`') == target => {
                out.push_str(text);
                rest = from_open.get(consumed..).unwrap_or("");
            }
            _ => {
                out.push('[');
                rest = from_open.get(1..).unwrap_or("");
            }
        }
    }
    out.push_str(rest);
    out.replace("**", "")
}

/// `(texto, alvo, bytes consumidos)` de um link `[texto](alvo)` no início de `from`.
fn parse_link(from: &str) -> Option<(&str, &str, usize)> {
    let _span = crate::trace_fn!("prompt::parse_link");

    let close = from.find("](")?;
    let text = from.get(1..close)?;
    let rest = from.get(close.saturating_add(2)..)?;
    let end = rest.find(')')?;
    let target = rest.get(..end)?;
    Some((
        text,
        target,
        close
            .saturating_add(2)
            .saturating_add(end)
            .saturating_add(1),
    ))
}

#[cfg(test)]
mod tests {
    use super::condense;

    #[test]
    fn redundant_link_keeps_the_target_once() {
        let text = condense("- **Regras**: [`docs/agent-rules.md`](docs/agent-rules.md).\n");
        assert_eq!(text, "- Regras: `docs/agent-rules.md`.\n");
    }

    #[test]
    fn a_link_with_its_own_text_is_preserved() {
        let text = condense("Ver [a política](plan/03.md) agora.\n");
        assert_eq!(text, "Ver [a política](plan/03.md) agora.\n");
    }

    #[test]
    fn blank_lines_collapse_and_output_is_deterministic() {
        let source = "# A\n\n\n\n- x\n";
        assert_eq!(condense(source), "# A\n\n- x\n");
        assert_eq!(condense(source), condense(source));
    }

    #[test]
    fn every_target_survives_the_condensation() {
        let source = "- [`a.md`](a.md)\n- [`b.md`](b.md)\n- [texto](c.md)\n";
        let text = condense(source);
        for target in ["a.md", "b.md", "c.md"] {
            assert!(text.contains(target), "{target} perdido em {text}");
        }
    }
}
