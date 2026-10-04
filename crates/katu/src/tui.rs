//! Comando `tui` (E10-T01/T02/T05): UI de terminal sobre o loop de turnos.
//!
//! A UI é **pura** (estado central, keymap, render em `katu-tui`); os efeitos vivem no
//! [`Kernel`] (thread do kernel) e no [`KernelClient`] desta borda. O runtime e o provider
//! pertencem ao kernel; a UI nunca fala com o modelo. Durante o turno, os deltas e as tools em
//! curso são reencaminhados **ao vivo** para o painel de atividade (E10-T05) através do `Painter`,
//! sem entrarem no log nem no transcript.

use katu_core::api::{Command, channel};
use katu_core::context::CompactionMode;
use katu_core::error::Error;
use katu_core::ports::Env as _;
use katu_core::provider::{ModelSpec, Provider, Thinking};
use katu_tui::{App, Update, run};

use crate::agent::{RunArgs, build_provider, default_base, default_model, open_runtime};
use crate::ports::{StdEnv, StdFs, StdProcess, SystemClock};
use crate::pricing;
use crate::report::Report;
use crate::runtime::Runtime;
use crate::tier::TierPolicy;

mod handler;

use crate::kernel::{Kernel, models_for, thinking_options};
use handler::KernelClient;

/// Corre a UI de terminal ligada ao loop de turnos.
pub(crate) fn run_tui(args: &RunArgs<'_>) -> Report {
    let _span = katu_core::trace_fn!("tui::run_tui");
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
    let wiring = match wire(args, &runtime, &env) {
        Ok(wiring) => wiring,
        Err(report) => return report,
    };
    let Wiring {
        provider,
        tiers,
        prices,
        model,
    } = wiring;

    // Estado inicial da UI (antes de mover o provider/runtime para o kernel).
    let mut app = App::new();
    app.apply_update(Update::Models(models_for(provider.as_ref(), &model)));
    app.apply_update(Update::ThinkingOptions(thinking_options(
        provider.as_ref(),
        &model,
    )));
    apply_initial(&mut app, &runtime);
    let granted_by = env
        .var("USER")
        .or_else(|| env.var("USERNAME"))
        .unwrap_or_else(|| "local".to_string());

    // K1/K2: o kernel vive na sua thread e é o dono do runtime/provider; a UI é cliente.
    let (bus, handle) = channel();
    let kernel = Kernel {
        runtime,
        provider: std::sync::Arc::from(provider),
        fs: &fs,
        process,
        env,
        model: ModelSpec::new(model),
        turn_model: None,
        tiers,
        prices,
        max_tokens: args.max_tokens,
        max_steps: args.max_steps,
    };
    let outcome = std::thread::scope(|scope| {
        let _kernel = scope.spawn(move || kernel.run(bus));
        // O cliente vive **dentro** do escopo: quando a UI sai, o canal fecha e a thread do kernel
        // termina — o `join` implícito do escopo nunca fica pendurado.
        let mut client = KernelClient::new(handle, granted_by);
        // Pensamento por omissão da config (E20-T17), aplicado como controlo logado no arranque.
        apply_thinking(&mut app, &client, args.thinking);
        run(app, &mut client, &clock)
    });
    match outcome {
        Ok(()) => Report::ok("tui", None),
        Err(error) => Report::failed("tui", &Error::io("<tui>", error)),
    }
}

/// Provider, *tiers*, preços e modelo resolvidos a partir das flags e do runtime.
struct Wiring {
    provider: Box<dyn Provider>,
    tiers: TierPolicy,
    prices: katu_providers::PriceTable,
    model: String,
}

/// Resolve a cablagem do comando, devolvendo o relatório de falha pronto a devolver.
fn wire(args: &RunArgs<'_>, runtime: &Runtime<'_>, env: &StdEnv) -> Result<Wiring, Report> {
    let _span = katu_core::trace_fn!("tui::wire");

    let base = args
        .base
        .map_or_else(|| default_base(args.provider).to_string(), str::to_string);
    let provider = build_provider(args.provider, &base, env, runtime.session_id())
        .map_err(|message| Report::failed("tui", &Error::invalid_input(message)))?;
    let tiers = TierPolicy::load()
        .map_err(|message| Report::failed("tui", &Error::invalid_input(message)))?;
    let prices = pricing::price_table()
        .map_err(|message| Report::failed("tui", &Error::invalid_input(message)))?;
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
    Ok(Wiring {
        provider,
        tiers,
        prices,
        model,
    })
}

/// Aplica o pensamento por omissão da config como controlo **logado** (E20-T17).
fn apply_thinking(app: &mut App, client: &KernelClient, thinking: Option<Thinking>) {
    let _span = katu_core::trace_fn!("tui::apply_thinking");

    if let Some(thinking) = thinking {
        for update in client.request(Command::SetThinking(thinking)) {
            app.apply_update(update);
        }
    }
}

/// Injeta o checkpoint de fase (próxima ação) no arranque (E10-T06), se houver.
fn apply_initial(app: &mut App, runtime: &Runtime<'_>) {
    let _span = katu_core::trace_fn!("tui::apply_initial");

    match runtime.checkpoint() {
        Ok(Some(checkpoint)) => app.apply_update(Update::NextAction(checkpoint.next_action)),
        Ok(None) => {}
        Err(error) => app.apply_update(Update::Error(error.to_string())),
    }
}
