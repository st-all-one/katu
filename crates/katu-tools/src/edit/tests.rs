//! Testes do `edit` (Q-07): atomicidade, ordem, ambiguidade e as âncoras que ensinam.

use super::{EditFileTool, FailureKind, Replacement, apply, nearest_anchors};
use katu_core::error::ToolOutcome;
use katu_core::kernel::Tool;
use katu_core::ports::{Fs, MemFs};
use katu_policy::{ResolvedPath, ToolArgs, ToolName, ToolUse};

const PATH: &str = "/work/src/lib.rs";

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

/// Ferramenta com uma substituição única (`let x = 1;` → `let x = 2;`).
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "teste: `dry_run` é o eixo do cenário"
)]
fn tool(fs: &MemFs, dry_run: bool) -> EditFileTool<'_> {
    EditFileTool {
        fs,
        replacements: vec![Replacement::new("let x = 1;", "let x = 2;")],
        dry_run,
    }
}

/// Ferramenta com várias substituições.
fn multi(fs: &MemFs, replacements: Vec<Replacement>) -> EditFileTool<'_> {
    EditFileTool {
        fs,
        replacements,
        dry_run: false,
    }
}

#[test]
fn patch_applies_and_reports_hashes() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    fs.write_atomic(std::path::Path::new(PATH), b"fn a() { let x = 1; }\n")?;
    let output = tool(&fs, false).execute(&use_()?);
    assert_eq!(output.outcome, ToolOutcome::Ok);
    let report = output.report.ok_or("sem relatório")?;
    let toon = report.to_toon();
    assert!(toon.contains("edit.patch\u{1f}"), "{toon}");
    assert!(
        toon.contains("path\u{1f}/work/src/lib.rs\nedits\u{1f}1\nhunks\u{1f}1\n"),
        "{toon}"
    );
    assert_eq!(
        fs.read(std::path::Path::new(PATH))?,
        b"fn a() { let x = 2; }\n"
    );
    Ok(())
}

#[test]
fn dry_run_does_not_write() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let original = b"fn a() { let x = 1; }\n";
    fs.write_atomic(std::path::Path::new(PATH), original)?;
    let output = tool(&fs, true).execute(&use_()?);
    assert!(
        output
            .report
            .is_some_and(|report| report.to_toon().contains("edit.dry-run"))
    );
    assert_eq!(fs.read(std::path::Path::new(PATH))?, original.to_vec());
    Ok(())
}

#[test]
fn ambiguous_patch_is_unavailable() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    fs.write_atomic(std::path::Path::new(PATH), b"let x = 1; let x = 1;\n")?;
    assert!(matches!(
        tool(&fs, false).execute(&use_()?).outcome,
        ToolOutcome::Unavailable { .. }
    ));
    Ok(())
}

#[test]
fn missing_patch_is_unavailable() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    fs.write_atomic(std::path::Path::new(PATH), b"nada aqui\n")?;
    assert!(matches!(
        tool(&fs, false).execute(&use_()?).outcome,
        ToolOutcome::Unavailable { .. }
    ));
    Ok(())
}

#[test]
fn noop_patch_reports_the_real_delta() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    fs.write_atomic(std::path::Path::new(PATH), b"fn a() { let x = 1; }\n")?;
    let tool = multi(&fs, vec![Replacement::new("let x = 1;", "let x = 1;")]);
    let output = tool.execute(&use_()?);
    let report = output.report.ok_or("sem relatório")?;
    let rendered = report.to_toon();
    // O delta é real: `hunks` não é um literal; sem alteração, são zero.
    assert!(
        rendered.contains("hunks\u{1f}0\nadded\u{1f}0\nremoved\u{1f}0\n"),
        "{rendered}"
    );
    Ok(())
}

#[test]
fn several_replacements_apply_in_order_in_one_call() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    fs.write_atomic(
        std::path::Path::new(PATH),
        b"fn a() { let x = 1; }\nfn b() { let y = 1; }\n",
    )?;
    let tool = multi(
        &fs,
        vec![
            Replacement::new("let x = 1;", "let x = 2;"),
            // Depende do resultado da primeira (aplica-se **por ordem**, não em paralelo).
            Replacement::new("let x = 2;", "let x = 3;"),
            Replacement::new("let y = 1;", "let y = 9;"),
        ],
    );
    let output = tool.execute(&use_()?);
    assert_eq!(output.outcome, ToolOutcome::Ok);
    let report = output.report.ok_or("sem relatório")?;
    let rendered = report.to_toon();
    assert!(rendered.contains("edits\u{1f}3\n"), "{rendered}");
    assert_eq!(
        fs.read(std::path::Path::new(PATH))?,
        b"fn a() { let x = 3; }\nfn b() { let y = 9; }\n"
    );
    Ok(())
}

/// **Atomicidade**: a substituição 3 não casa ⇒ as duas primeiras **não** são gravadas.
#[test]
fn a_failing_replacement_writes_nothing() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let original = b"fn a() { let x = 1; }\nfn b() { let y = 1; }\n";
    fs.write_atomic(std::path::Path::new(PATH), original)?;
    let tool = multi(
        &fs,
        vec![
            Replacement::new("let x = 1;", "let x = 2;"),
            Replacement::new("let y = 1;", "let y = 2;"),
            Replacement::new("let z = 1;", "let z = 2;"),
        ],
    );
    let output = tool.execute(&use_()?);
    assert!(matches!(output.outcome, ToolOutcome::Unavailable { .. }));
    assert_eq!(
        fs.read(std::path::Path::new(PATH))?,
        original.to_vec(),
        "nada pode ser gravado quando uma substituição falha"
    );
    let rendered = output.report.ok_or("sem relatório")?.to_toon();
    assert!(rendered.contains("edit.rejected"), "{rendered}");
    assert!(rendered.contains("reason\u{1f}not-found\n"), "{rendered}");
    // A recusa diz **qual** falhou (índice 2, base 0) e quantas eram.
    assert!(
        rendered.contains("edit\u{1f}2\nedits\u{1f}3\n"),
        "{rendered}"
    );
    Ok(())
}

/// A recusa por ambiguidade diz **quantas** vezes casou e **onde**.
#[test]
fn an_ambiguous_replacement_names_the_lines() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    fs.write_atomic(
        std::path::Path::new(PATH),
        b"fn a() { let x = 1; }\nfn b() { let x = 1; }\n",
    )?;
    let output = tool(&fs, false).execute(&use_()?);
    let rendered = output.report.ok_or("sem relatório")?.to_toon();
    assert!(rendered.contains("reason\u{1f}ambiguous\n"), "{rendered}");
    assert!(rendered.contains("1: fn a() { let x = 1; }"), "{rendered}");
    assert!(rendered.contains("2: fn b() { let x = 1; }"), "{rendered}");
    Ok(())
}

/// A recusa por "não encontrado" devolve as **âncoras únicas mais próximas** (Q-08).
#[test]
fn a_missing_replacement_suggests_the_nearest_unique_anchor()
-> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    fs.write_atomic(
        std::path::Path::new(PATH),
        b"fn alpha() {\n    let allowed = 1;\n}\nfn beta() {\n    let allowed = 2;\n}\n",
    )?;
    let tool = multi(
        &fs,
        vec![Replacement::new("let allowed = 3;", "let ok = 3;")],
    );
    let output = tool.execute(&use_()?);
    let rendered = output.report.ok_or("sem relatório")?.to_toon();
    // `let allowed = 1;` é único; `let allowed = 2;` também — ambos partilham o prefixo da sonda.
    assert!(rendered.contains("\u{1e}anchors\n"), "{rendered}");
    assert!(rendered.contains("2: let allowed = 1;"), "{rendered}");
    assert!(rendered.contains("5: let allowed = 2;"), "{rendered}");
    assert!(rendered.contains("remedy\u{1f}"), "{rendered}");
    assert!(
        !rendered.contains("\u{1e}next\n"),
        "sem duplicação: {rendered}"
    );
    Ok(())
}

#[test]
fn the_nearest_anchor_ignores_repeated_lines() {
    let text = "let x = 1;\nlet x = 1;\nlet allowed = 2;\n";
    let anchors = nearest_anchors(text, "let allowed = 3;");
    assert_eq!(anchors, vec!["3: let allowed = 2;".to_string()]);
}

#[test]
fn apply_is_deterministic_and_leaves_the_original_untouched() {
    let text = "a\nb\n";
    let replacements = vec![Replacement::new("a", "x")];
    let first = apply(text, &replacements).unwrap_or_default();
    assert_eq!(first, "x\nb\n");
    assert_eq!(apply(text, &replacements).unwrap_or_default(), first);
    assert_eq!(text, "a\nb\n");
}

#[test]
fn an_empty_replacement_list_is_a_noop() {
    assert_eq!(apply("a\n", &[]).unwrap_or_default(), "a\n");
}

#[test]
fn a_failure_carries_the_index_and_the_kind() {
    let failure = apply(
        "a\n",
        &[Replacement::new("a", "b"), Replacement::new("z", "y")],
    )
    .err();
    assert_eq!(
        failure,
        Some(super::EditFailure {
            index: 1,
            kind: FailureKind::NotFound,
            anchors: Vec::new(),
        })
    );
}
