#![allow(clippy::print_stdout, reason = "xtask dev-only: relatório do portão")]
//! `gate:prompt` (Q-20): a composição do prompt **medida e travada**.
//!
//! Reconstrói o que entra no pedido ao modelo — `AGENTS.md` condensado, catálogo de skills, prime e
//! as schemas JSON das tools — a partir das **funções de produção** (nunca de uma cópia), compara
//! com orçamentos declarados e publica o artefacto cru. Nasceu da medição de §1.5 do
//! `OPTIMIZATION_PLAN.md`: o `bench/e18/REPORT.md` §121 admitia que a composição dos 3265 tokens
//! não estava medida, e nada travava uma regressão.
//!
//! O artefacto traz também a **baseline** (a renderização anterior a Q-05/Q-19: descrição completa
//! das skills e `AGENTS.md` cru), calculada da mesma fonte, para que o ganho publicado seja
//! derivável do artefacto e não de uma memória (DF5).
//!
//! `--write` regrava `bench/e18/prompt/raw.json` (determinístico); sem ele, o gate só verifica.
//! Os tokens usam o rácio **medido** de Q-01 (`katu_core::context::tokens_from_bytes`), o mesmo do
//! orçamento do kernel — uma segunda constante seria *drift*.

use std::fs;
use std::path::Path;

use katu_core::context::{BYTES_PER_TOKEN_MILLI, prime, tokens_from_bytes};
use katu_core::ports::{Fs as _, MemFs};
use katu_core::prompt::condense;
use katu_core::provider::ToolDef;
use katu_core::skill::{SKILL_DIRS, Skill, catalog, discover, relative_path};
use katu_tools::schema::{tool_defs, wire_json};
use serde_json::{Value, json};

/// Orçamento por parte, em bytes (teto; exceder **falha** o gate).
///
/// O teto das tools subiu de 5 100 para 5 200 B em Q-07: o par `old`/`new` passou a **lista** (edição
/// multi-bloco atómica), o que custou **+162 B** (+45 tokens) de wire — o preço de ensinar a forma
/// sem gastar um parâmetro novo. Antes desta correção o teto das tools era impresso mas **não**
/// verificado (um teto que não trava não é um teto).
const BUDGETS: [(&str, usize); 4] = [
    ("AGENTS.md", 1_300),
    ("catálogo de skills", 1_300),
    ("prime", 1_400),
    ("tools (wire JSON)", 5_200),
];

/// Orçamento do `system` (AGENTS.md + prime + catálogo de skills), em tokens estimados.
const BUDGET_SYSTEM_TOKENS: usize = 2_200;

/// Artefacto cru da composição.
const ARTIFACT: &str = "bench/e18/prompt/raw.json";

/// A composição do prompt e a sua baseline.
struct Composition {
    /// `AGENTS.md` condensado (o que o modelo vê; Q-19).
    agents: usize,
    /// `AGENTS.md` cru (baseline).
    agents_baseline: usize,
    /// Catálogo de skills como sai hoje (bytes).
    skills: usize,
    /// Catálogo de skills na renderização **anterior a Q-05** (bytes), da mesma fonte.
    skills_baseline: usize,
    /// Prime (bytes).
    prime: usize,
    /// Schemas JSON das tools na forma do wire (bytes).
    tools: usize,
    /// Schemas JSON sem as tools de uso raro (sonda de Q-18; bytes).
    tools_core: usize,
}

impl Composition {
    /// `system` = `AGENTS.md` + prime + catálogo (bytes).
    const fn system(&self) -> usize {
        self.agents
            .saturating_add(self.prime)
            .saturating_add(self.skills)
    }

    /// `system` na baseline (bytes).
    const fn system_baseline(&self) -> usize {
        self.agents_baseline
            .saturating_add(self.prime)
            .saturating_add(self.skills_baseline)
    }

    /// Prompt total = `system` + tools (bytes).
    const fn total(&self) -> usize {
        self.system().saturating_add(self.tools)
    }

    /// Prompt total na baseline (bytes).
    const fn total_baseline(&self) -> usize {
        self.system_baseline().saturating_add(self.tools)
    }

    /// Parte pelo nome, para o relatório.
    fn part(&self, name: &str) -> usize {
        match name {
            "AGENTS.md" => self.agents,
            "catálogo de skills" => self.skills,
            "prime" => self.prime,
            _ => self.tools,
        }
    }
}

/// Ganho relativo de `baseline` → `current` (0 se não houver baseline).
fn gain(current: usize, baseline: usize) -> f64 {
    if baseline == 0 {
        return 0.0;
    }
    let current = f64::from(u32::try_from(current).unwrap_or(u32::MAX));
    let baseline = f64::from(u32::try_from(baseline).unwrap_or(u32::MAX));
    let delta = (baseline - current) / baseline;
    (delta * 10_000.0).round() / 10_000.0
}

/// Mede a composição do prompt a partir das funções de produção.
fn measure() -> Result<Composition, String> {
    // A raiz **canónica** é a que o `Runtime` usa (`discover_root`): o caminho do `SKILL.md` é
    // renderizado relativo a ela, logo os bytes não dependem do *checkout*.
    let root = fs::canonicalize(".").map_err(|err| format!("canonicalizando a raiz: {err}"))?;
    let root = root.as_path();
    let agents = fs::read_to_string(root.join("AGENTS.md")).unwrap_or_default();
    let skills_fs = mirror_skills(root)?;
    // O objetivo não entra na medição: com objetivo vazio a ordem é pelo nome (determinística) e o
    // que se mede — os **bytes** — não depende dela.
    let skills = discover(&skills_fs, root);
    let skills_text = catalog(&skills, "", root);
    let defs = tool_defs();
    let wire_bytes = |subset: &[&ToolDef]| {
        let joined = subset
            .iter()
            .map(|tool| wire_json(tool).to_string())
            .collect::<Vec<_>>()
            .join(",");
        format!("[{joined}]").len()
    };
    let all: Vec<&ToolDef> = defs.iter().collect();
    // Sonda de Q-18: o que se pouparia omitindo as tools de uso raro. `plan`/`memory` **não**
    // entram: o prime instrui o modelo a usá-las (omiti-las seria incoerente).
    let core: Vec<&ToolDef> = all
        .iter()
        .copied()
        .filter(|tool| !["move", "trash"].contains(&tool.name.as_str()))
        .collect();
    Ok(Composition {
        agents: condense(agents.trim()).trim().len(),
        agents_baseline: agents.trim().len(),
        skills: skills_text.len(),
        skills_baseline: skills_baseline(&skills, &skills_text, root),
        prime: prime().len(),
        tools: wire_bytes(&all),
        tools_core: wire_bytes(&core),
    })
}

/// Bytes do catálogo na renderização **anterior a Q-05**.
///
/// A forma é `- {nome} — {descrição completa} ({caminho relativo})\n`. O cabeçalho e o caminho são
/// os mesmos de hoje; só a descrição mudou (Q-05 encurtou-a), pelo que o ganho isola essa mudança.
fn skills_baseline(skills: &[Skill], current: &str, root: &Path) -> usize {
    let header = current
        .lines()
        .next()
        .map_or(0, |line| line.len().saturating_add(1));
    skills.iter().fold(header, |total, skill| {
        // `- ` (2) + ` — ` (5, o travessão é multi-byte) + ` (` (2) + `)\n` (2).
        total
            .saturating_add(skill.name.len())
            .saturating_add(skill.description.len())
            .saturating_add(relative_path(&skill.path, root).len())
            .saturating_add(11)
    })
}

/// Espelha `.agents/skill{,s}/*/SKILL.md` num [`MemFs`] para reusar [`discover`] sem reimplementar a
/// descoberta (uma só fonte para as regras de nomes, dedup e fail-open).
fn mirror_skills(root: &Path) -> Result<MemFs, String> {
    let fs = MemFs::new();
    for dir in SKILL_DIRS {
        let base = root.join(dir);
        let Ok(entries) = fs::read_dir(&base) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let file = path.join("SKILL.md");
            let Ok(bytes) = fs::read(&file) else {
                continue;
            };
            fs.create_dir_all(&path)
                .map_err(|err| format!("espelhando {}: {err}", path.display()))?;
            fs.write_atomic(&file, &bytes)
                .map_err(|err| format!("espelhando {}: {err}", file.display()))?;
        }
    }
    Ok(fs)
}

/// Verifica os orçamentos e (com `--write`) publica o artefacto.
pub(crate) fn gate_prompt(args: &[String]) -> Result<(), String> {
    let write = args.iter().any(|arg| arg == "--write");
    let composition = measure()?;

    let mut violations: Vec<String> = Vec::new();
    for (name, budget) in BUDGETS {
        let bytes = composition.part(name);
        println!(
            "  {name:24} {bytes:>6} B  {:>5} tok  (teto {budget} B)",
            tokens_from_bytes(bytes)
        );
        if bytes > budget {
            violations.push(format!("{name}: {bytes} B excede o teto de {budget} B"));
        }
    }
    let system_tokens = tokens_from_bytes(composition.system());
    let system_bytes = composition.system();
    println!(
        "  {:24} {system_bytes:>6} B  {system_tokens:>5} tok  (teto {BUDGET_SYSTEM_TOKENS} tok)",
        "system"
    );
    println!(
        "  {:24} {:>6} B  (baseline; ganho {:.1} %)",
        "AGENTS.md cru",
        composition.agents_baseline,
        gain(composition.agents, composition.agents_baseline) * 100.0
    );
    if system_tokens > BUDGET_SYSTEM_TOKENS {
        violations.push(format!(
            "system: {system_tokens} tok excede o teto de {BUDGET_SYSTEM_TOKENS} tok"
        ));
    }

    if write {
        let payload = artifact(&composition);
        let mut text = serde_json::to_string_pretty(&payload)
            .map_err(|err| format!("serializando a composição: {err}"))?;
        text.push('\n');
        fs::write(ARTIFACT, text).map_err(|err| format!("escrevendo {ARTIFACT}: {err}"))?;
        println!("  artefacto: {ARTIFACT}");
    }

    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "gate:prompt falhou (a composição do prompt regrediu; §1.5 do OPTIMIZATION_PLAN):\n  {}",
            violations.join("\n  ")
        ))
    }
}

/// O artefacto cru: partes, baseline e ganhos deriváveis.
fn artifact(composition: &Composition) -> Value {
    let tokens = |bytes: usize| tokens_from_bytes(bytes);
    json!({
        "protocol": "bench/e18/prompt/PROTOCOL.md",
        "bytes_per_token_milli": BYTES_PER_TOKEN_MILLI,
        "parts": [
            {"name": "AGENTS.md", "bytes": composition.agents, "tokens": tokens(composition.agents)},
            {"name": "catálogo de skills", "bytes": composition.skills, "tokens": tokens(composition.skills)},
            {"name": "prime", "bytes": composition.prime, "tokens": tokens(composition.prime)},
            {"name": "tools (wire JSON)", "bytes": composition.tools, "tokens": tokens(composition.tools)},
        ],
        "system_bytes": composition.system(),
        "system_tokens": tokens(composition.system()),
        "total_bytes": composition.total(),
        "total_tokens": tokens(composition.total()),
        "baseline": {
            "agents_bytes": composition.agents_baseline,
            "skills_bytes": composition.skills_baseline,
            "system_bytes": composition.system_baseline(),
            "system_tokens": tokens(composition.system_baseline()),
            "total_tokens": tokens(composition.total_baseline()),
        },
        "probes": {
            "tools_core_bytes": composition.tools_core,
            "tools_core_gain": gain(composition.tools_core, composition.tools),
        },
        "gains": {
            "agents": gain(composition.agents, composition.agents_baseline),
            "skills": gain(composition.skills, composition.skills_baseline),
            "system": gain(composition.system(), composition.system_baseline()),
            "total": gain(composition.total(), composition.total_baseline()),
        },
    })
}
