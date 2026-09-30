//! Comando `tui` (E10-T01/T02/T05): UI de terminal sobre o loop de turnos.
//!
//! A UI é **pura** (estado central, keymap, render em `katu-tui`); os efeitos vivem no
//! [`AgentHandler`] desta borda. O runtime e o provider pertencem ao handler; a UI nunca fala com o
//! modelo. Durante o turno, os deltas e as tools em curso são reencaminhados **ao vivo** para o
//! painel de atividade (E10-T05) através do `Painter`, sem entrarem no log nem no transcript.

use katu_core::context::CompactionMode;
use katu_core::diag::{Level, events};
use katu_core::error::Error;
use katu_core::provider::{ModelSpec, Provider, Thinking};
use katu_tui::{App, Update, run};

use crate::agent::{RunArgs, build_provider, default_base, default_model, open_runtime};
use crate::ports::{StdEnv, StdFs, StdProcess, SystemClock};
use crate::pricing;
use crate::report::Report;
use crate::runtime::Runtime;
use crate::tier::TierPolicy;

mod control;
mod handler;
mod transcript;
mod trash;
mod verify;

use handler::AgentHandler;

/// Corre a UI de terminal ligada ao loop de turnos.
pub(crate) fn run_tui(args: &RunArgs<'_>) -> Report {
    let fs = StdFs;
    let clock = SystemClock;
    let env = StdEnv;
    let process = StdProcess;
    let start = std::env::current_dir().unwrap_or_default();
    let mut runtime = match open_runtime(&fs, &clock, &start, "cli: tui", args.resume) {
        Ok(runtime) => runtime,
        Err(error) => return Report::failed("tui", &Error::from(error)),
    };
    if args.compact {
        runtime.set_compaction(CompactionMode::Enabled);
    }
    let base = args
        .base
        .map_or_else(|| default_base(args.provider).to_string(), str::to_string);
    let provider = match build_provider(args.provider, &base, &env, runtime.session_id()) {
        Ok(provider) => provider,
        Err(message) => return Report::failed("tui", &Error::invalid_input(message)),
    };
    let tiers = match TierPolicy::load() {
        Ok(tiers) => tiers,
        Err(message) => return Report::failed("tui", &Error::invalid_input(message)),
    };
    let prices = match pricing::price_table() {
        Ok(prices) => prices,
        Err(message) => return Report::failed("tui", &Error::invalid_input(message)),
    };
    let model = args.model.map_or_else(
        || {
            tiers.model_for(
                provider.as_ref(),
                runtime.phase(),
                default_model(args.provider),
            )
        },
        str::to_string,
    );
    let models = models_for(provider.as_ref(), &model);
    let thinking = control::thinking_options(provider.as_ref(), &model);
    let mut app = App::new();
    app.apply_update(Update::Models(models));
    app.apply_update(Update::ThinkingOptions(thinking));
    apply_initial(&mut app, &runtime);
    let mut handler = AgentHandler {
        runtime,
        provider,
        fs: &fs,
        process,
        env,
        model: ModelSpec::new(model),
        tiers,
        prices,
        max_tokens: args.max_tokens,
        max_steps: args.max_steps,
    };
    // Pensamento por omissão da config (E20-T17), aplicado como controlo **logado** no arranque.
    apply_thinking(&mut app, &mut handler, args.thinking);
    match run(app, &mut handler, &clock) {
        Ok(()) => Report::ok("tui", None),
        Err(error) => Report::failed("tui", &Error::io("<tui>", error)),
    }
}

/// Aplica o pensamento por omissão da config como controlo **logado** (E20-T17).
fn apply_thinking(app: &mut App, handler: &mut AgentHandler<'_>, thinking: Option<Thinking>) {
    if let Some(thinking) = thinking {
        for update in handler.set_thinking(thinking) {
            app.apply_update(update);
        }
    }
}

/// Injeta o checkpoint de fase (próxima ação) no arranque (E10-T06), se houver.
fn apply_initial(app: &mut App, runtime: &Runtime<'_>) {
    match runtime.checkpoint() {
        Ok(Some(checkpoint)) => app.apply_update(Update::NextAction(checkpoint.next_action)),
        Ok(None) => {}
        Err(error) => app.apply_update(Update::Error(error.to_string())),
    }
}

/// Modelos oferecidos no seletor da TUI (E12-T02/T10): do **endpoint**, com queda no catálogo.
///
/// Tenta a descoberta ao vivo (`dynamic_models`); se falhar ou vier vazia, usa o catálogo estático.
/// O default vem primeiro para o índice zero coincidir com o modelo do arranque.
fn models_for(provider: &dyn Provider, default: &str) -> Vec<String> {
    let discovered = provider.dynamic_models().unwrap_or_default();
    katu_core::event!(
        Level::Debug,
        events::PROVIDER_MODELS,
        "source" => if discovered.is_empty() { "catalog" } else { "endpoint" },
        "count" => discovered.len(),
    );
    let listed = if discovered.is_empty() {
        provider.models()
    } else {
        discovered
    };
    let mut models: Vec<String> = listed
        .into_iter()
        .filter(|model| model.as_str() != default)
        .collect();
    models.insert(0, default.to_string());
    models
}
