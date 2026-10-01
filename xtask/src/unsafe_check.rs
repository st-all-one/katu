//! `check-unsafe` (E13-T04) — `unsafe` proibido em cada crate puro, com **uma** exceção registada.
//!
//! A política de memória (ADR 0016) é `forbid`, não `deny`: `unsafe` é um **erro de compilação** em
//! todo o crate puro. A única exceção autorizada é o `kill(2)` do **grupo** de processos
//! (E07-T04), porque `std` não expõe wrapper seguro; vive em [`EXCEPTIONS`], num só ficheiro, e o
//! crate que a usa declara `deny` (não `forbid`). Este check confirma a declaração de cada crate,
//! recusa `allow(unsafe_code)` fora da lista e exige que a lista esteja **exatamente** esgotada.
//!
//! O CI corre também Miri (`cargo miri test`) e `cargo machete` (deps mortas).
//!
//! A segunda excepção (medição de alocações, E18-T10) é o **único** `unsafe` que não toca memória
//! de produção: vive num alvo de integração, e o `katu-tui` mantém `forbid` sem escape hatch.

use std::fs;
use std::path::{Path, PathBuf};

use crate::walk::{collect_by_extension, is_foreign_root};

/// Declaração que torna `unsafe` um erro de compilação, sem exceções possíveis.
const FORBID: &str = "forbid(unsafe_code)";

/// Declaração alternativa, obrigatória em crates com exceção (permite `allow` local).
const DENY: &str = "deny(unsafe_code)";

/// Escape hatch que este check só aceita dentro da lista de exceções (comparado sem espaços).
const ESCAPE: &str = "#[allow(unsafe_code";

/// Exceções autorizadas: `(ficheiro, motivo)`. Cada uma é um ponto de `unsafe` **contado** (§42):
/// o gate exige que a lista esteja exactamente esgotada, para que uma tercera não apareça calada.
const EXCEPTIONS: [(&str, &str); 2] = [
    (
        "crates/katu/src/ports/process.rs",
        "kill(2) do grupo de processos no timeout (E07-T04): `std` não expõe wrapper seguro",
    ),
    (
        "crates/katu/tests/render_alloc.rs",
        "medição de alocações do render (E18-T10): contar alocações exige `unsafe impl GlobalAlloc`; só encaminha para `System`, tem testes próprios no mesmo ficheiro e vive num alvo de teste",
    ),
];

/// Crates com exceção: declaram `deny` (não `forbid`) para permitir o `allow` local.
const EXCEPTION_CRATES: [&str; 1] = ["crates/katu"];

/// Ponto de entrada de `check-unsafe`.
pub(crate) fn check_unsafe() -> Result<(), String> {
    let mut violations: Vec<String> = Vec::new();
    for root in crate_roots()? {
        if !declaration_ok(&root) {
            violations.push(format!(
                "{}: falta `#![{FORBID}]` (crates com exceção: `#![{DENY}]`)",
                root.display()
            ));
        }
    }
    let mut files: Vec<PathBuf> = Vec::new();
    collect_by_extension(Path::new("crates"), "rs", &mut files)?;
    let mut escapes: usize = 0;
    for file in &files {
        let text =
            fs::read_to_string(file).map_err(|err| format!("lendo {}: {err}", file.display()))?;
        let found = count_escapes(&text);
        if found > 0 {
            escapes = escapes.saturating_add(found);
            if !is_authorized(file) {
                violations.push(format!("{}: `{ESCAPE})]` não autorizado", file.display()));
            }
        }
    }
    if escapes != EXCEPTIONS.len() {
        violations.push(format!(
            "esperava exatamente {} `{ESCAPE}` autorizado(s) ({}), encontrei {escapes}",
            EXCEPTIONS.len(),
            EXCEPTIONS
                .iter()
                .map(|(file, _)| *file)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "check-unsafe falhou (ADR 0016):\n  {}",
            violations.join("\n  ")
        ))
    }
}

/// `true` se a raiz do crate declara a política correta (`forbid`, ou `deny` se tiver exceção).
fn declaration_ok(root: &Path) -> bool {
    let exception = EXCEPTION_CRATES
        .iter()
        .any(|crate_root| root == Path::new(*crate_root));
    if exception {
        crate_declares(root, DENY) && !crate_declares(root, FORBID)
    } else {
        crate_declares(root, FORBID)
    }
}

/// `true` se a raiz do crate declara `needle` em `lib.rs`/`main.rs`.
fn crate_declares(root: &Path, needle: &str) -> bool {
    ["src/lib.rs", "src/main.rs"].iter().any(|relative| {
        fs::read_to_string(root.join(relative)).is_ok_and(|text| text.contains(needle))
    })
}

/// `true` se o ficheiro pertence a uma exceção autorizada.
fn is_authorized(file: &Path) -> bool {
    EXCEPTIONS
        .iter()
        .any(|(allowed, _)| file.ends_with(allowed))
}

/// Diretórios de crate puro (não-foreign) sob `crates/`.
fn crate_roots() -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir("crates").map_err(|err| format!("lendo crates: {err}"))?;
    let mut roots: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|err| format!("lendo entrada: {err}"))?;
        let path = entry.path();
        if path.is_dir() && !is_foreign_root(&path) && path.join("Cargo.toml").exists() {
            roots.push(path);
        }
    }
    roots.sort();
    Ok(roots)
}

/// `true` se a linha contém o escape hatch.
fn count_escapes(text: &str) -> usize {
    // Um escape hatch é um **atributo**, não uma citação: primeiro descarta linhas de comentário
    // (contar a documentação que *fala* do `allow` tornava o gate manipulável por prosa), e só
    // depois compacta os espaços em branco, para que a forma partida em várias linhas
    // (`#[allow(\n    unsafe_code,`…) conte como uma.
    let code: String = text
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    code.matches(ESCAPE).count()
}

#[cfg(test)]
mod tests {
    use super::{ESCAPE, FORBID, count_escapes, is_authorized};

    #[test]
    fn prose_about_an_escape_hatch_does_not_count() {
        let text =
            "//! registado em `#[allow(unsafe_code)]`\n#[allow(unsafe_code, reason = \"x\")]\n";
        assert_eq!(
            count_escapes(text),
            1,
            "um comentário contou como escape hatch"
        );
    }

    #[test]
    fn declaration_and_escape_are_detected() {
        assert!("#![forbid(unsafe_code)]".contains(FORBID));
        assert_eq!(count_escapes("#![forbid(unsafe_code)]"), 0);
        assert_eq!(count_escapes("#[allow(unsafe_code, reason = \"x\")]"), 1);
        assert_eq!(
            count_escapes("#[allow(\n    unsafe_code,\n    reason = \"x\"\n)]"),
            1
        );
        assert_eq!(count_escapes(ESCAPE), 1);
    }

    #[test]
    fn only_the_registered_file_is_authorized() {
        assert!(is_authorized(std::path::Path::new(
            "crates/katu/src/ports/process.rs"
        )));
        assert!(!is_authorized(std::path::Path::new(
            "crates/katu/src/tui.rs"
        )));
        assert!(!is_authorized(std::path::Path::new(
            "crates/katu-core/src/lib.rs"
        )));
    }
}
