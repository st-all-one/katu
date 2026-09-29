//! Inspetor de `argv` (E07-T02): classificação **determinística**, **sem regex**.
//!
//! A decisão nunca olha para a prosa do comando. Reconhece, por **igualdade de strings**, os
//! programas que são interpretadores (`sh`, `bash`, `python`, …), as flags que invocam código
//! inline (`-c`, `-e`, `--eval`) e as flags que aninham outro comando (`find -exec`).
//!
//! Um interpretador com código inline é **opaco**: não se pode verificar o que corre, pelo que uma
//! capacidade por programa **não** basta. O mesmo vale para flags destrutivas ou aninhadas.

use serde::{Deserialize, Serialize};

use crate::paths::ResolvedArgv;

/// Interpretadores conhecidos (comparados pelo *basename*, exato).
const INTERPRETERS: &[&str] = &[
    "sh", "bash", "zsh", "dash", "ksh", "fish", "csh", "tcsh", "ash", "python", "python2",
    "python3", "pypy", "perl", "ruby", "node", "deno", "php", "lua", "awk", "gawk", "mawk", "sed",
];

/// Flags que invocam **código inline** num interpretador.
const INLINE_FLAGS: &[&str] = &["-c", "-e", "--eval", "-E"];

/// Flags que **aninham** outro comando (executam um programa a partir do `argv`).
const NESTED_FLAGS: &[&str] = &["-exec", "-execdir", "-ok", "-okdir", "--exec"];

/// Flags destrutivas reconhecidas (alteram/removem sem confirmação).
const DESTRUCTIVE_FLAGS: &[&str] = &["-delete", "-remove", "--delete"];

/// Programas que **acedam à rede** (comparados pelo *basename*, exato). E07-T05.
const NETWORK_PROGRAMS: &[&str] = &[
    "curl", "wget", "ssh", "scp", "sftp", "rsync", "nc", "netcat", "ncat", "telnet", "ping", "ftp",
    "socat",
];

/// Natureza do programa (evita dois `bool` ortogonais no mesmo struct).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ProgramKind {
    /// Programa comum (não interpretador).
    Command,
    /// Interpretador sem código inline (`python script.py`).
    Interpreter,
    /// Interpretador com código inline (`bash -c …`): **opaco**.
    InlineInterpreter,
}

/// Classificação determinística de um `argv` resolvido.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArgvInspection {
    /// Programa base (basename do primeiro argumento).
    pub program: String,
    /// Natureza do programa.
    pub kind: ProgramKind,
    /// Flags destrutivas presentes no `argv`.
    pub destructive: Vec<String>,
    /// Flags que aninham outro comando (`find -exec`, …).
    pub nested: Vec<String>,
    /// `true` se o programa acede à rede (`curl`, `ssh`, …).
    pub network: bool,
    /// Host extraído do `argv` (autoridade de URL ou `user@host`), quando reconhecível.
    pub host: Option<String>,
}

impl ArgvInspection {
    /// `true` se o programa é um interpretador (com ou sem código inline).
    #[must_use]
    pub fn is_interpreter(&self) -> bool {
        !matches!(self.kind, ProgramKind::Command)
    }

    /// `true` se a execução é **opaca** (interpretador com código inline, ou comando aninhado):
    /// não se pode verificar o que corre, pelo que uma capacidade por programa **não** basta.
    #[must_use]
    pub fn is_opaque(&self) -> bool {
        matches!(self.kind, ProgramKind::InlineInterpreter) || !self.nested.is_empty()
    }

    /// `true` se a capacidade por programa pode destrancar a execução (verificável, não destrutiva
    /// e **não de rede** — a rede exige `Capability::Net`, E07-T05).
    #[must_use]
    pub fn is_plain(&self) -> bool {
        !self.is_opaque() && self.destructive.is_empty() && !self.network
    }
}

/// Inspeciona um `argv` resolvido (puro, sem I/O).
#[must_use]
pub fn inspect(argv: &ResolvedArgv) -> ArgvInspection {
    let program = basename(argv.program());
    let interpreter = INTERPRETERS.contains(&program.as_str());
    let mut inline = false;
    let mut destructive = Vec::new();
    let mut nested = Vec::new();
    for arg in argv.as_slice().iter().skip(1) {
        let arg = arg.as_str();
        if INLINE_FLAGS.contains(&arg) {
            inline = true;
        }
        if DESTRUCTIVE_FLAGS.contains(&arg) {
            destructive.push(arg.to_string());
        }
        if NESTED_FLAGS.contains(&arg) {
            nested.push(arg.to_string());
        }
    }
    let kind = match (interpreter, inline) {
        (false, _) => ProgramKind::Command,
        (true, false) => ProgramKind::Interpreter,
        (true, true) => ProgramKind::InlineInterpreter,
    };
    let network = NETWORK_PROGRAMS.contains(&program.as_str());
    let host = if network {
        host_of(argv.as_slice())
    } else {
        None
    };
    ArgvInspection {
        program,
        kind,
        destructive,
        nested,
        network,
        host,
    }
}

/// Extrai o host de um `argv` (autoridade de URL ou `user@host`), sem I/O.
fn host_of(args: &[String]) -> Option<String> {
    for arg in args.iter().skip(1) {
        if let Some((_, rest)) = arg.split_once("://") {
            let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
            let host = authority.rsplit('@').next().unwrap_or(authority);
            let host = host.split(':').next().unwrap_or(host);
            if !host.is_empty() {
                return Some(host.to_string());
            }
        }
        if let Some((_, host)) = arg.split_once('@') {
            let host = host.split(':').next().unwrap_or(host);
            if !host.is_empty() {
                return Some(host.to_string());
            }
        }
    }
    None
}

/// Basename de um programa (sem diretório), sem I/O.
fn basename(program: &str) -> String {
    program.rsplit('/').next().unwrap_or(program).to_string()
}

#[cfg(test)]
mod tests {
    use super::{ArgvInspection, ProgramKind, inspect};
    use crate::paths::ResolvedArgv;

    fn argv(parts: &[&str]) -> Result<ResolvedArgv, crate::PolicyError> {
        ResolvedArgv::new(parts.iter().map(|part| (*part).to_string()).collect())
    }

    fn inspect_parts(parts: &[&str]) -> Result<ArgvInspection, crate::PolicyError> {
        Ok(inspect(&argv(parts)?))
    }

    #[test]
    fn plain_command_is_verifiable() -> Result<(), crate::PolicyError> {
        let inspection = inspect_parts(&["ls", "-la"])?;
        assert_eq!(inspection.program, "ls");
        assert_eq!(inspection.kind, ProgramKind::Command);
        assert!(!inspection.is_interpreter());
        assert!(inspection.is_plain());
        Ok(())
    }

    #[test]
    fn basename_is_used_for_interpreters() -> Result<(), crate::PolicyError> {
        let inspection = inspect_parts(&["/usr/bin/bash", "-c", "echo hi"])?;
        assert_eq!(inspection.program, "bash");
        assert_eq!(inspection.kind, ProgramKind::InlineInterpreter);
        assert!(inspection.is_interpreter());
        assert!(inspection.is_opaque());
        Ok(())
    }

    #[test]
    fn find_delete_and_exec_are_not_plain() -> Result<(), crate::PolicyError> {
        let delete = inspect_parts(&["find", ".", "-delete"])?;
        assert_eq!(delete.destructive, vec!["-delete".to_string()]);
        assert!(!delete.is_plain());

        let exec = inspect_parts(&["find", ".", "-exec", "rm", "{}", ";"])?;
        assert_eq!(exec.nested, vec!["-exec".to_string()]);
        assert!(exec.is_opaque());
        Ok(())
    }

    #[test]
    fn shell_metacharacters_are_opaque() -> Result<(), crate::PolicyError> {
        let inspection = inspect_parts(&["sh", "-c", "cd x && rm -rf /"])?;
        assert!(inspection.is_opaque());
        Ok(())
    }

    #[test]
    fn quoted_program_is_not_normalized() -> Result<(), crate::PolicyError> {
        let inspection = inspect_parts(&["r''m", "-rf", "/"])?;
        assert_eq!(inspection.program, "r''m");
        assert_eq!(inspection.kind, ProgramKind::Command);
        assert!(inspection.is_plain());
        Ok(())
    }

    #[test]
    fn network_programs_are_flagged_with_a_host() -> Result<(), crate::PolicyError> {
        let curl = inspect_parts(&["curl", "https://example.com/a?b=1"])?;
        assert!(curl.network);
        assert_eq!(curl.host.as_deref(), Some("example.com"));
        assert!(!curl.is_plain(), "a rede exige `Capability::Net`");

        let ssh = inspect_parts(&["ssh", "user@host.example:2222"])?;
        assert!(ssh.network);
        assert_eq!(ssh.host.as_deref(), Some("host.example"));

        let local = inspect_parts(&["ls", "-la"])?;
        assert!(!local.network);
        assert_eq!(local.host, None);
        assert!(local.is_plain());
        Ok(())
    }
}
