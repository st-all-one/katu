//! Conformidade do registo de esquema (gate contra secções sem esquema).

use super::{Mode, registry, spec, validate};

#[test]
fn registry_is_well_formed() {
    let issues = validate();
    assert!(issues.is_empty(), "{issues:?}");
}

#[test]
fn core_sections_are_registered() {
    let names = [
        "r",
        "k",
        "sym",
        "m",
        "tool",
        "checks",
        "symbols",
        "flags",
        "imports",
        "hits",
        "clusters.hits",
        "hunks",
        "hunks.lines",
        "entries",
        "files",
        "features",
        "argv",
        "next",
        "text",
        "stdout",
        "stderr",
    ];
    for name in names {
        assert!(spec(name).is_some(), "secção `{name}` sem esquema");
    }
}

#[test]
fn literal_sections_have_no_columns() {
    for table in registry() {
        if table.mode == Mode::Literal {
            assert!(
                table.cols.is_empty(),
                "`{}` literal com colunas",
                table.name
            );
        }
    }
}
