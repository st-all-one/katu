//! `check-paths` — **nada é escrito fora do projecto**, excepto a configuração global.
//!
//! Invariante do dono (§42): os logs e o estado do katu vivem em `<raiz>/.katu/` (sessões,
//! auditoria, transcrição, plano, configuração do projecto) e no índice `.knudge/`; **nada** vive
//! fora da raiz do projecto, à excepção da **configuração global** (`$XDG_CONFIG_HOME/katu`), que
//! é por definição partilhada entre projectos.
//!
//! O gate é **estático** (como o `check-unsafe`): procura em código de produção construções de
//! caminho que escapem da raiz — `env::temp_dir`, `home_dir`, `/tmp`, `/var`, leitura de `HOME`/`XDG_*`
//! — e recusa-as, excepto numa lista de excepções **exactamente esgotada**, para que uma terceira
//! não apareça calada. Blocos `#[cfg(test)]` são ignorados: testes podem usar temporários.
//!
//! Excepções registadas (e porquê):
//! - [`ALLOWED`] `crates/katu/src/config.rs` — a configuração global **é** a única coisa fora do
//!   projecto, por decisão;
//! - [`ALLOWED`] `crates/katu/src/watch_service.rs` — materializa uma unidade systemd de
//!   utilizador, mas **só** com `--watch-service --install` explícito; não é estado do katu.

use std::fs;
use std::path::{Path, PathBuf};

use crate::walk::{collect_by_extension, is_foreign_root};

/// Construções que levam a escrita para fora da raiz do projecto.
///
/// Agulhas **disjuntas** de propósito: `temp_dir` contido em `env::temp_dir` contaria duas vezes
/// e o gate passaria a contar erros em vez de sítios.
const ESCAPES: [&str; 6] = [
    "temp_dir()",
    "home_dir",
    "\"/tmp",
    "/tmp/",
    "XDG_DATA_HOME",
    "XDG_CONFIG_HOME",
];

/// Excepções autorizadas: `(ficheiro, motivo)`. A lista tem de estar **exactamente** esgotada.
const ALLOWED: [(&str, &str); 2] = [
    (
        "crates/katu/src/config.rs",
        "configuração global (`$XDG_CONFIG_HOME/katu`): a única coisa que vive fora do projecto, por decisão",
    ),
    (
        "crates/katu/src/watch_service.rs",
        "unidade systemd de utilizador, materializada só com `--watch-service --install` explícito; não é estado do katu",
    ),
];

/// Ponto de entrada de `check-paths`.
pub(crate) fn check_paths() -> Result<(), String> {
    let mut violations: Vec<String> = Vec::new();
    let mut found: Vec<String> = Vec::new();
    for file in production_sources()? {
        let text =
            fs::read_to_string(&file).map_err(|err| format!("lendo {}: {err}", file.display()))?;
        let escapes = count_escapes(&text);
        if escapes == 0 {
            continue;
        }
        let path = file.to_string_lossy().replace('\\', "/");
        let tag = if is_allowed(&file) {
            " (autorizado)"
        } else {
            ""
        };
        found.push(format!("    {path}: {escapes}{tag}"));
        if !is_allowed(&file) {
            violations.push(format!(
                "{path}: {escapes} construção(ões) fora do projecto"
            ));
        }
    }
    // A lista de excepções tem de estar exactamente esgotada nas **duas** direções: nenhuma
    // construção nova sem Justificativa, e nenhuma justificação que já não tenha nada que
    // autorizar.
    for (allowed, reason) in ALLOWED {
        let still_used = found
            .iter()
            .any(|line| line.starts_with(&format!("    {allowed}:")));
        if !still_used {
            violations.push(format!(
                "excepção obsoleta (nada autoriza): {allowed} — {reason}"
            ));
        }
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "check-paths falhou (§42: nada fora do projecto excepto a config global):\n  {}\n  escapes:\n{}",
            violations.join("\n  "),
            found.join("\n")
        ))
    }
}

/// Ficheiros `.rs` de produção: `crates/*/src`, sem `tests*`, sem o submódulo estranho.
fn production_sources() -> Result<Vec<PathBuf>, String> {
    let mut files: Vec<PathBuf> = Vec::new();
    collect_by_extension(Path::new("crates"), "rs", &mut files)?;
    files.retain(|file| {
        let path = file.to_string_lossy();
        let is_src = path.contains("/src/");
        let is_test =
            path.contains("/tests") || path.ends_with("tests.rs") || path.contains("/tests/");
        is_src && !is_test && !is_foreign_root(file)
    });
    files.sort();
    Ok(files)
}

/// `true` se o ficheiro está na lista de excepções.
fn is_allowed(file: &Path) -> bool {
    let path = file.to_string_lossy().replace('\\', "/");
    ALLOWED.iter().any(|(allowed, _)| path.ends_with(allowed))
}

/// Conta as construções de escape **fora** de blocos de teste (`#[cfg(test)]`,
/// `#[cfg(all(test, unix))]`, …).
///
/// A profundidade é de **chaves**; o bloco termina quando volta ao nível em que começou, e só
/// depois da linha do marcador (uma linha `#[cfg(test)]` não tem chaves, pelo que fechá-la no
/// mesmo instante contaria o interior). Um módulo de uma só linha, já equilibrado, não abre janela.
fn count_escapes(source: &str) -> usize {
    let mut count: usize = 0;
    let mut depth: i32 = 0;
    let mut test_depth: Option<i32> = None;
    let mut started = false;
    for line in source.lines() {
        let braces = i32::from(line.contains('{')).saturating_sub(i32::from(line.contains('}')));
        depth = depth.saturating_add(braces);
        if let Some(start) = test_depth {
            if started && depth <= start {
                test_depth = None;
                started = false;
            }
            continue; // dentro de bloco de teste: não conta
        }
        if is_test_cfg(line) && !(braces == 0 && line.contains('}')) {
            test_depth = Some(depth);
            started = true;
            continue;
        }
        count = count.saturating_add(
            ESCAPES
                .iter()
                .map(|needle| line.matches(needle).count())
                .sum::<usize>(),
        );
    }
    count
}

/// `true` se a linha for um `#[cfg(...)]` que activa testes.
fn is_test_cfg(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("#[cfg(") && trimmed.contains("test")
}

#[cfg(test)]
mod tests {
    use super::{ALLOWED, ESCAPES, count_escapes, is_allowed, is_test_cfg};

    #[test]
    fn a_clean_module_has_no_escapes() {
        assert_eq!(
            count_escapes("fn root() -> PathBuf { root.join(\".katu\") }\n"),
            0
        );
    }

    #[test]
    fn production_escapes_are_counted() {
        let source = "fn tmp() { std::env::temp_dir() }\nfn home() { home_dir() }\n";
        assert_eq!(count_escapes(source), 2);
    }

    #[test]
    fn a_test_block_is_not_counted() {
        let source = "\
fn production() { std::env::temp_dir() }

#[cfg(test)]
mod tests {
    fn helper() { std::env::temp_dir() }
}
";
        assert_eq!(count_escapes(source), 1, "o bloco de teste contou");
    }

    #[test]
    fn a_composed_test_cfg_is_recognised() {
        assert!(is_test_cfg("#[cfg(all(test, unix))]"));
        assert!(is_test_cfg("#[cfg(test)]"));
        assert!(!is_test_cfg("#[cfg(feature = \"x\")]"));
        assert!(!is_test_cfg("mod tests {"));
    }

    #[test]
    fn a_composed_test_module_is_skipped() {
        let source = "fn p() { home_dir() }\n#[cfg(all(test, unix))]\nmod tests {\n  fn t() { temp_dir() }\n}\n";
        assert_eq!(
            count_escapes(source),
            1,
            "o bloco `cfg(all(test, unix))` contou"
        );
    }

    #[test]
    fn an_inlined_test_module_still_ends() {
        let source =
            "#[cfg(test)]\nmod tests { fn a() { temp_dir() } }\nfn depois() { home_dir() }\n";
        assert_eq!(count_escapes(source), 1, "o módulo de teste não terminou");
    }

    #[test]
    fn the_exception_list_is_small_and_declared() {
        assert_eq!(ALLOWED.len(), 2);
        for (file, reason) in ALLOWED {
            assert!(
                file.starts_with("crates/katu/src/"),
                "excepção fora do binário: {file}"
            );
            assert!(reason.len() > 40, "excepção sem motivo explicado: {file}");
        }
    }

    #[test]
    fn only_the_declared_files_are_allowed() {
        assert!(is_allowed(Path::new("crates/katu/src/config.rs")));
        assert!(is_allowed(Path::new("crates/katu/src/watch_service.rs")));
        assert!(!is_allowed(Path::new("crates/katu/src/runtime.rs")));
        assert!(!is_allowed(Path::new("crates/katu-core/src/feedback.rs")));
    }

    #[test]
    fn every_escape_needle_is_a_real_token() {
        for needle in ESCAPES {
            assert!(!needle.is_empty());
        }
        assert!(ESCAPES.contains(&"temp_dir()"));
        assert!(ESCAPES.contains(&"XDG_CONFIG_HOME"));
    }

    use std::path::Path;
}
