//! A/B do caminho `memory.write` + gate (P-03). **Dev-only**: corre com `--ignored` e escreve o
//! artefacto em `KATU_MEMORY_OUT`.
//!
//! Mede, em **release**, cada metade do caminho §42 da escrita de memória com uma base de
//! conhecimento real (notas escritas pelo próprio adaptador):
//!
//! - `pre_write` — a decisão de dedup (índice + peneira de postings + `dice` sobre os candidatos);
//! - `record` — o commit isolado (sem `pre_write` antes: paga o índice do zero);
//! - `gate` — `enforce_memory_write` inteiro (pré-validação + política + executor), com o commit a
//!   **reusar** o índice que a pré-validação construiu (P-03);
//! - `gate_legacy` — réplica congelada do gate anterior (`Knudge::write_context()`, que re-lê todas
//!   as notas do disco a cada commit);
//! - `parts` — atribuição: índice reconstruído do store, proposta e escrita da nota.
//!
//! A pergunta: o commit re-ler o store inteiro (em vez de reusar o índice que o `pre_write` já
//! construiu) custa o quê, em função do número de notas? Sem o número, é preferência.

use std::path::{Path, PathBuf};
use std::time::Instant;

use katu_core::error::ToolOutcome;
use katu_core::kernel::{MemoryWriteRequest, State, Tool, ToolOutput, enforce_memory_write};
use katu_core::memory::{Anchor, Memory, NoteType, PreWriteOutcome, PreWriteReq};
use katu_core::ports::Env;
use katu_policy::{ControlId, ResolvedPath, RuleSet, ToolName, ToolUse};
use katu_tools::write::WriteNoteTool;
use knudge_core::Knudge;
use knudge_core::write::{WriteContext, propose, write};
use serde_json::json;

use crate::memory::KnudgeMemory;
use crate::memory::translate;
use crate::ports::StdEnv;

/// Regras reais do protocolo de memória (o gate é o mesmo da produção).
const MEMORY_POLICY: &str = include_str!("../../../../../policy/memory.toml");

/// Base de conhecimento medida (o setup escreve as notas pelo próprio adaptador).
const NOTES: [u32; 3] = [100, 500, 1_000];

/// Repetições por medição (mediana).
const REPS: u32 = 20;

/// Ponto de entrada do bench (`KATU_MEMORY_OUT=... cargo test -p katu --bin katu -- --ignored ab_memory_write`).
///
/// Escreve **só** para o ficheiro indicado: o caminho de produção não pode imprimir (o gate
/// `check-diag` recusa `println!` em `crates/`, e um bench não é exceção).
#[test]
#[ignore = "bench dev-only: escreve o artefacto em KATU_MEMORY_OUT"]
fn ab_memory_write() -> Result<(), Box<dyn std::error::Error>> {
    // O var vem da porta `Env` (o `std::env::var` é não determinístico e está banido).
    let out = StdEnv
        .var("KATU_MEMORY_OUT")
        .ok_or("KATU_MEMORY_OUT ausente")?;
    let mut rows = Vec::new();
    for notes in NOTES {
        rows.push(measure(notes)?);
    }
    let artifact = json!({
        "schema": "katu.bench.memory.v1",
        "machine": { "os": std::env::consts::OS, "arch": std::env::consts::ARCH },
        "reps": REPS,
        "rows": rows,
        "limits": [
            "base sintética: afirmações únicas de vocabulário parecido, escritas pelo adaptador",
            "o commit inclui a escrita do ficheiro da nota e do evento (em /tmp, tmpfs)",
            "o gate_legacy é uma réplica congelada do commit anterior, não uma segunda implementação",
        ],
    });
    std::fs::write(
        &out,
        format!("{}\n", serde_json::to_string_pretty(&artifact)?),
    )?;
    Ok(())
}

/// Mede uma base com `notes` notas.
///
/// Cada repetição usa uma afirmação **nova**: é o caminho real (o `pre_write` decide `Create` e o
/// commit cria a nota), não o caminho de duplicata, que mede outra coisa.
fn measure(notes: u32) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let root = temp_root(notes)?;
    let memory = KnudgeMemory::open(&root)?;
    // Réplica congelada do caminho anterior, sobre o **mesmo** store (mesmo layout de conhecimento).
    let legacy_kd = Knudge::builder()
        .root(&root)
        .knowledge_dir(".katu/knowledge")
        .open()?;
    // A base é escrita **direto no store** (sem dedup): o dedup fundiria frases do mesmo
    // vocabulário e a base deixaria de ter `notes` notas — o setup não é a medição.
    for index in 0..notes {
        let draft = translate::draft(
            &statement(index),
            NoteType::Decision,
            &format!("corpo da nota {index}"),
            None,
        );
        let note = draft.to_note(legacy_kd.now_ms())?;
        legacy_kd.store().write(&note)?;
    }
    let mut rep = 0_u32;
    let mut next = || {
        rep = rep.saturating_add(1);
        request(notes, rep)
    };
    // Que caminho está a ser medido? O `pre_write` decide; sem isto, o leitor não sabe se o commit
    // criou ou fundiu.
    let decision = decision(&memory, &next())?;
    let pre_write = median(REPS, || {
        let req = next();
        memory.pre_write(&req).map(|_| ()).map_err(Into::into)
    })?;
    let record = median(REPS, || {
        let req = next();
        memory.record(&req).map(|_| ()).map_err(Into::into)
    })?;
    // O gate é medido com um adaptador **fresco** por repetição: em produção o índice é invalidado
    // pela escrita anterior, e um adaptador quente deixaria o caminho antigo a beneficiar de um
    // cache que não invalidou (medição contaminada). O custo de abrir entra nos dois lados.
    let gate = median(REPS, || {
        let req = next();
        let fresh = KnudgeMemory::open(&root)?;
        run_gate(&fresh, &legacy_kd, &req, Commit::Production)
    })?;
    let gate_legacy = median(REPS, || {
        let req = next();
        let fresh = KnudgeMemory::open(&root)?;
        run_gate(&fresh, &legacy_kd, &req, Commit::Legacy)
    })?;
    let parts = parts(&legacy_kd, &root, notes)?;
    std::fs::remove_dir_all(&root).ok();
    Ok(json!({
        "notes": notes,
        "decision": decision,
        "pre_write_us": pre_write,
        "record_us": record,
        "gate_us": gate,
        "gate_legacy_us": gate_legacy,
        "gain_ratio": share(gate_legacy.saturating_sub(gate), gate_legacy),
        "parts": parts,
    }))
}

/// Vocabulário denso, como o de um projeto real: as notas partilham termos.
///
/// Cada afirmação tem quatro palavras rotativas e um número próprio, para que a base não seja uma
/// única nota fundida pelo dedup.
const WORDS: [&str; 30] = [
    "cache", "indice", "fila", "motor", "roteador", "coletor", "janela", "espelho", "cursor",
    "muralha", "filtro", "bucha", "farol", "ancora", "portico", "viga", "calha", "bussola",
    "dique", "moinho", "prensa", "sonda", "turbina", "valvula", "zarcao", "ceramica", "grafite",
    "linha", "malha", "nos",
];

/// Quatro palavras rotativas da **metade** `half` do vocabulário, determinísticas por índice.
///
/// A base e as afirmações de medição usam metades **disjuntas**: o `dice` fica bem abaixo do limiar
/// de dedup (uma medição rejeitada pelo dedup não chegaria a correr o commit e mediria um gate
/// negado), sem perder o vocabulário denso que faz a peneira de postings ter candidatos.
fn phrase(index: u32, half: usize) -> String {
    let n = usize::try_from(index).unwrap_or(0);
    let size = WORDS.len().checked_div(2).unwrap_or(1);
    let base = half.checked_mul(size).unwrap_or(0);
    let word = |slot: usize| -> &'static str {
        let position = base.wrapping_add(
            n.wrapping_mul(7)
                .wrapping_add(slot.wrapping_mul(11))
                .checked_rem(size)
                .unwrap_or(0),
        );
        WORDS.get(position).copied().unwrap_or("termo")
    };
    format!(
        "{} {} {} {} usa a politica propria",
        word(0),
        word(1),
        word(2),
        word(3)
    )
}

/// Decide o caminho medido e **recusa** uma medição que o dedup não deixaria correr.
///
/// Uma afirmação rejeitada não chega ao commit (o gate nega por falta de capacidade), pelo que a
/// medição deixaria de ser do caminho real.
fn decision(
    memory: &KnudgeMemory,
    req: &PreWriteReq,
) -> Result<&'static str, Box<dyn std::error::Error>> {
    let outcome = memory.pre_write(req)?;
    match outcome {
        PreWriteOutcome::Create => Ok("create"),
        PreWriteOutcome::Merge { .. } => Ok("merge"),
        PreWriteOutcome::Reject { duplicate, .. } => Err(format!(
            "a afirmação de medição é duplicata de {}",
            duplicate.as_str()
        )
        .into()),
        _ => Ok("outro"),
    }
}

/// Afirmação da base (única por índice).
fn statement(index: u32) -> String {
    format!("{} variante {index}", phrase(index, 0))
}

/// Pedido de medição (afirmação nova, nunca duplicada).
fn request(notes: u32, rep: u32) -> PreWriteReq {
    PreWriteReq {
        // Três tokens **únicos** por repetição: sem eles, duas medições seriam quase idênticas (o
        // `dice` passa de 0,75) e o dedup fundiria/rejeitaria a segunda — o commit não correria.
        statement: format!(
            "za{notes}q{rep} zb{notes}r{rep} zc{notes}s{rep} usa a politica propria"
        ),
        note_type: NoteType::Decision,
        anchor: None,
        body: "corpo novo".to_string(),
    }
}

/// Atribuição do custo: reconstruir o índice do store, propor e escrever a nota.
fn parts(
    kd: &Knudge,
    root: &Path,
    notes: u32,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let open_us = median(REPS, || {
        KnudgeMemory::open(root).map(|_| ()).map_err(Into::into)
    })?;
    let index_us = median(REPS, || kd.index().map(|_| ()).map_err(Into::into))?;
    let draft = translate::draft(
        &request(notes, 9_999).statement,
        NoteType::Decision,
        "corpo",
        None,
    );
    let thresholds = kd.thresholds()?;
    let index = kd.index()?;
    let propose_us = median(REPS, || {
        propose(&index, &draft, &thresholds)
            .map(|_| ())
            .map_err(Into::into)
    })?;
    let write_us = match kd.store().list_ids()?.into_iter().next() {
        Some(id) => median(REPS, || {
            let note = kd.store().read(&id)?;
            kd.store().write(&note)?;
            Ok(())
        })?,
        None => 0,
    };
    Ok(json!({
        "open_us": open_us,
        "index_from_store_us": index_us,
        "propose_us": propose_us,
        "store_write_us": write_us,
    }))
}

/// Réplica do commit anterior a P-03: `write_context()` reconstrói o índice a partir do store.
fn record_legacy(kd: &Knudge, req: &PreWriteReq) -> Result<(), Box<dyn std::error::Error>> {
    // O mesmo construtor de rascunho do adaptador (não uma segunda tradução).
    let draft = translate::draft(
        &req.statement,
        req.note_type,
        &req.body,
        req.anchor.as_ref().map(Anchor::as_str),
    );
    let thresholds = kd.thresholds()?;
    let ctx = WriteContext::new(kd.store(), kd.events(), kd.index()?, kd.now_ms());
    write(&ctx, &draft, &thresholds)?;
    Ok(())
}

/// Qual executor o gate usa na medição.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Commit {
    /// O executor de produção ([`WriteNoteTool`]).
    Production,
    /// A réplica congelada do caminho anterior a P-03.
    Legacy,
}

/// Executor do commit pela réplica congelada (o caminho anterior a P-03).
struct Legacy<'a> {
    kd: &'a Knudge,
    req: &'a PreWriteReq,
}

impl Tool for Legacy<'_> {
    fn name(&self) -> ToolName {
        ToolName::MemoryWrite
    }

    fn execute(&self, _use_: &ToolUse) -> ToolOutput {
        let _span = katu_core::trace_fn!("bench::legacy_execute");

        match record_legacy(self.kd, self.req) {
            Ok(()) => ToolOutput::ok(),
            Err(_) => ToolOutput::outcome(ToolOutcome::Unavailable {
                control: ControlId::new("memory"),
                rule_id: None,
            }),
        }
    }
}

/// Corre o gate completo (pré-validação + política + executor), sem tocar no log.
///
/// O executor é o **mesmo** da produção ([`WriteNoteTool`]) ou a réplica congelada do caminho
/// anterior, conforme `legacy`: em ambos os casos o commit corre de facto.
fn run_gate(
    memory: &KnudgeMemory,
    kd: &Knudge,
    req: &PreWriteReq,
    commit: Commit,
) -> Result<(), Box<dyn std::error::Error>> {
    let rules = RuleSet::from_toml(MEMORY_POLICY)?;
    let cwd = ResolvedPath::from_canonical("/work")?;
    let production = WriteNoteTool {
        memory,
        req: req.clone(),
    };
    let replica = Legacy { kd, req };
    let tool: &dyn Tool = match commit {
        Commit::Legacy => &replica,
        Commit::Production => &production,
    };
    let mut state = State::initial();
    state.completed_tools.insert(ToolName::MemoryRecall);
    let dispatch = enforce_memory_write(
        &state,
        MemoryWriteRequest {
            cwd: &cwd,
            req,
            memory,
            rules: &rules,
            now_millis: 1_000,
            tool,
        },
    )?;
    if !dispatch.ran() {
        return Err(format!(
            "o gate não executou o commit ({:?}, regra {:?}): a medição não seria do caminho real",
            dispatch.outcome(),
            dispatch
                .outcome()
                .rule_id()
                .map(katu_policy::RuleId::as_str)
        )
        .into());
    }
    // O commit tem de ter **efeito**: os dois executores mapeiam um erro em `Unavailable` sem
    // propagar, pelo que um commit falhado mediria outra coisa.
    if dispatch.outcome() != ToolOutcome::Ok {
        return Err(format!("o commit não correu: {:?}", dispatch.outcome()).into());
    }
    Ok(())
}

/// Mediana de `reps` execuções, em microssegundos.
#[allow(
    clippy::disallowed_methods,
    reason = "instrumento dev-only: o relógio monotónico é o objeto da medição"
)]
fn median(
    reps: u32,
    mut run: impl FnMut() -> Result<(), Box<dyn std::error::Error>>,
) -> Result<u64, Box<dyn std::error::Error>> {
    let mut samples = Vec::with_capacity(usize::try_from(reps).unwrap_or(0));
    for _ in 0..reps {
        let start = Instant::now();
        run()?;
        samples.push(u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX));
    }
    samples.sort_unstable();
    Ok(samples.get(samples.len() / 2).copied().unwrap_or(0))
}

/// Fração de `total` que `part` representa (4 casas).
fn share(part: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    let ratio = f64::from(u32::try_from(part).unwrap_or(u32::MAX))
        / f64::from(u32::try_from(total).unwrap_or(u32::MAX));
    (ratio * 10_000.0).round() / 10_000.0
}

/// Raiz temporária única.
fn temp_root(notes: u32) -> Result<PathBuf, std::io::Error> {
    let path = std::env::temp_dir().join(format!("katu-mem-bench-{}-{notes}", std::process::id()));
    std::fs::remove_dir_all(&path).ok();
    std::fs::create_dir_all(&path)?;
    Ok(path)
}
