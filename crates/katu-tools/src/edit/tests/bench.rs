//! A/B do `edit` multi-bloco (Q-07): chamadas, bytes do payload e **atomicidade** — determinístico.
//!
//! A pergunta: um refactor canónico de 5 *hunks* custa menos com **uma** chamada atómica do que com
//! 5 chamadas separadas? O que é determinístico (e por isso medível sem modelo) é o número de
//! chamadas, os bytes que o modelo tem de escrever e o estado do ficheiro quando uma falha a meio.
//!
//! Corre-se com `KATU_EDIT_OUT=$PWD/bench/e18/edit/raw.json cargo test -p katu-tools --lib
//! -- --ignored --nocapture ab_multi_block_edit`; o artefacto é publicado e `bench/published.toml`
//! cita-o (DF5). `check-diag` proíbe `println!` em `crates/`: o número sai para **ficheiro**.

use std::path::Path;

use katu_core::evidence::to_f64;
use katu_core::kernel::Tool;
use katu_core::ports::{Fs, MemFs};
use katu_policy::{ResolvedPath, ToolArgs, ToolName, ToolUse};
use serde_json::json;

use crate::edit::{EditFileTool, Replacement};

/// Ficheiro canónico do refactor: 5 sítios com contexto distinto (cada âncora é única).
pub(super) const FILE: &str = "\
fn one() {
    let allowed = policy.authorize(&use_)?;
    run(allowed);
}

fn two() {
    let allowed = policy.authorize(&use_)?;
    run(allowed);
}

fn three() {
    let allowed = policy.authorize(&use_)?;
    run(allowed);
}

fn four() {
    let allowed = policy.authorize(&use_)?;
    run(allowed);
}

fn five() {
    let allowed = policy.authorize(&use_)?;
    run(allowed);
}
";

/// Caminho do ficheiro do cenário.
const PATH: &str = "/work/src/lib.rs";

/// O refactor: 5 substituições, cada uma com o contexto da sua função (âncora única).
pub(super) fn replacements() -> Vec<Replacement> {
    ["one", "two", "three", "four", "five"]
        .iter()
        .map(|name| {
            Replacement::new(
                format!("fn {name}() {{\n    let allowed = policy.authorize(&use_)?;"),
                format!("fn {name}() {{\n    let allowed = policy.check(&use_)?;"),
            )
        })
        .collect()
}

/// Razão segura (`0` quando o denominador é zero).
fn ratio(after: usize, before: usize) -> f64 {
    to_f64(u64::try_from(after).unwrap_or(u64::MAX))
        / to_f64(u64::try_from(before.max(1)).unwrap_or(1))
}

/// Bytes do payload de **uma** chamada (`arguments` do wire), como o modelo os escreve.
fn call_bytes(path: &str, replacements: &[Replacement]) -> Result<usize, serde_json::Error> {
    let olds: Vec<&str> = replacements.iter().map(|r| r.old.as_str()).collect();
    let news: Vec<&str> = replacements.iter().map(|r| r.new.as_str()).collect();
    let single = olds.len() == 1;
    let arguments = if single {
        json!({ "path": path, "old": olds.first(), "new": news.first() })
    } else {
        json!({ "path": path, "old": olds, "new": news })
    };
    Ok(serde_json::to_string(&arguments)?.len())
}

/// Uso resolvido da tool (o mesmo para as duas formas).
fn use_() -> Result<ToolUse, katu_policy::PolicyError> {
    let path = ResolvedPath::from_canonical(PATH)?;
    Ok(ToolUse {
        name: ToolName::Edit,
        args: ToolArgs::Edit { path: path.clone() },
        resolved_paths: vec![path.clone()],
        argv: None,
        cwd: path,
    })
}

/// Executa uma chamada e devolve o conteúdo final do ficheiro.
fn call(fs: &MemFs, replacements: Vec<Replacement>) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let tool = EditFileTool {
        fs,
        replacements,
        dry_run: false,
    };
    let _output = tool.execute(&use_()?);
    Ok(fs.read(Path::new(PATH))?)
}

/// Mede as duas formas e serializa o artefacto do A/B.
pub(super) fn measure() -> Result<String, Box<dyn std::error::Error>> {
    let hunks = replacements();
    let count = hunks.len();
    let before_bytes: usize = hunks
        .iter()
        .map(|replacement| call_bytes(PATH, std::slice::from_ref(replacement)))
        .collect::<Result<Vec<usize>, _>>()?
        .iter()
        .sum();
    let after_bytes = call_bytes(PATH, &hunks)?;
    let (final_before, final_after) = both_shapes(&hunks)?;
    let (partial, atomic) = on_failure(&hunks)?;

    let calls_gain = 100.0 * (1.0 - ratio(1, count));
    let bytes_gain = 100.0 * (1.0 - ratio(after_bytes, before_bytes.max(1)));
    let value = json!({
        "schema": "katu.bench.edit.v1",
        "question": "atomic_multi_block_edit",
        "hunks": count,
        "calls": { "before": count, "after": 1, "gain_pct": calls_gain },
        "call_payload_bytes": {
            "before": before_bytes,
            "after": after_bytes,
            "gain_pct": bytes_gain,
        },
        "log_events": { "before": count.saturating_mul(2), "after": 2 },
        "identical_final_state": final_before == final_after,
        "final_bytes": final_after.len(),
        "on_failure": {
            "before_partial_bytes": partial.len(),
            "after_partial_bytes": atomic.len(),
            "before_left_the_file_in_a_partial_state": partial != FILE.as_bytes(),
            "after_left_the_file_untouched": atomic == FILE.as_bytes(),
        },
        "criterion_pct": 20.0,
        "criterion_met": calls_gain >= 20.0,
        "caveat": "o número de *turnos* depende do modelo (com B-02 já pode agrupar as N chamadas num passo): o que é determinístico é o nº de chamadas, os bytes do payload, os eventos de log e a atomicidade; o A/B de turnos com o modelo não corre localmente (o modelo local não emite tool calls nativas)",
    });
    Ok(serde_json::to_string_pretty(&value)?)
}

/// Par `(antes, depois)`: o conteúdo do ficheiro em cada forma.
type Pair = (Vec<u8>, Vec<u8>);

/// Aplica o refactor pelas duas formas e devolve o conteúdo final de cada uma.
fn both_shapes(hunks: &[Replacement]) -> Result<Pair, Box<dyn std::error::Error>> {
    // Forma antiga: uma chamada por hunk (o ficheiro é lido e gravado N vezes).
    let fs_before = MemFs::new();
    fs_before.write_atomic(Path::new(PATH), FILE.as_bytes())?;
    let mut final_before = Vec::new();
    for replacement in hunks {
        final_before = call(&fs_before, vec![replacement.clone()])?;
    }
    // Forma nova: uma chamada com todos os hunks, atómica.
    let fs_after = MemFs::new();
    fs_after.write_atomic(Path::new(PATH), FILE.as_bytes())?;
    let final_after = call(&fs_after, hunks.to_vec())?;
    Ok((final_before, final_after))
}

/// O pior caso: a **3.ª** âncora não existe (a forma antiga para com 2 de 5 aplicadas).
fn on_failure(hunks: &[Replacement]) -> Result<Pair, Box<dyn std::error::Error>> {
    let mut broken = hunks.to_vec();
    let failing = Replacement::new(
        "fn three() {\n    let allowed = policy.missing(&use_)?;",
        "fn three() {\n    let ok = 1;",
    );
    if let Some(slot) = broken.get_mut(2) {
        *slot = failing;
    }
    let fs_partial = MemFs::new();
    fs_partial.write_atomic(Path::new(PATH), FILE.as_bytes())?;
    let mut partial = Vec::new();
    for replacement in &broken {
        let next = call(&fs_partial, vec![replacement.clone()])?;
        if next == partial {
            // A chamada falhou (nada mudou): é aqui que a forma antiga **para**.
            break;
        }
        partial = next;
    }
    let fs_atomic = MemFs::new();
    fs_atomic.write_atomic(Path::new(PATH), FILE.as_bytes())?;
    let atomic = call(&fs_atomic, broken)?;
    Ok((partial, atomic))
}

/// A/B publicado: escreve o artefacto em `KATU_EDIT_OUT` (opt-in) e afirma as invariantes do plano.
#[test]
#[ignore = "A/B de Q-07: escreve o artefacto em KATU_EDIT_OUT (não é asserção de CI)"]
fn ab_multi_block_edit() -> Result<(), Box<dyn std::error::Error>> {
    let json = measure()?;
    if let Some(path) = std::env::var_os("KATU_EDIT_OUT") {
        std::fs::write(path, format!("{json}\n"))?;
    }
    let value: serde_json::Value = serde_json::from_str(&json)?;
    let flag = |key: &str| value.get(key).and_then(serde_json::Value::as_bool);
    assert_eq!(
        flag("identical_final_state"),
        Some(true),
        "as duas formas têm de produzir o mesmo ficheiro"
    );
    let on_failure = value.get("on_failure");
    assert_eq!(
        on_failure
            .and_then(|value| value.get("after_left_the_file_untouched"))
            .and_then(serde_json::Value::as_bool),
        Some(true),
        "a forma atómica não pode gravar nada quando uma substituição falha"
    );
    Ok(())
}

/// O invariante do plano como teste de CI: a forma atómica **nunca** deixa o ficheiro a meio.
#[test]
fn the_multi_block_form_is_atomic_and_equivalent() -> Result<(), Box<dyn std::error::Error>> {
    let value: serde_json::Value = serde_json::from_str(&measure()?)?;
    assert_eq!(
        value
            .get("identical_final_state")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        value
            .get("on_failure")
            .and_then(|value| value.get("after_left_the_file_untouched"))
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    Ok(())
}
