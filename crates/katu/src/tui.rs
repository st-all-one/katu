//! Comando `tui` (E10-T01/T02/T05): UI de terminal sobre o loop de turnos.
//!
//! A UI é **pura** (estado central, keymap, render em `katu-tui`); os efeitos vivem no
//! [`AgentHandler`] desta borda. O runtime e o provider pertencem ao handler; a UI nunca fala com o
//! modelo. Durante o turno, os deltas e as tools em curso são reencaminhados **ao vivo** para o
//! painel de atividade (E10-T05) através do `Painter`, sem entrarem no log nem no transcript.

use katu_core::diag::{Level, events};
use katu_core::error::Error;
use katu_core::ports::Env;
use katu_core::provider::{ModelSpec, Provider};
use katu_tui::{App, ChallengePrompt, Command, Handler, Live, Painter, Update, run};

use crate::agent::{
    Activity, ActivitySink, Approval, ApprovalPrompt, Ports, RunArgs, SYSTEM, TurnOptions,
    TurnRequest, build_provider, default_base, default_model, run_turn_with,
};
use crate::ports::{StdEnv, StdFs, StdProcess, SystemClock};
use crate::report::Report;
use crate::runtime::Runtime;

/// Corre a UI de terminal ligada ao loop de turnos.
pub(crate) fn run_tui(args: &RunArgs<'_>) -> Report {
    let fs = StdFs;
    let clock = SystemClock;
    let env = StdEnv;
    let process = StdProcess;
    let start = std::env::current_dir().unwrap_or_default();
    let runtime = match Runtime::open(&fs, &clock, &start, "cli: tui") {
        Ok(runtime) => runtime,
        Err(error) => return Report::failed("tui", &Error::from(error)),
    };
    let model = args
        .model
        .map_or_else(|| default_model(args.provider).to_string(), str::to_string);
    let base = args
        .base
        .map_or_else(|| default_base(args.provider).to_string(), str::to_string);
    let provider = match build_provider(args.provider, &base, &env, runtime.session_id()) {
        Ok(provider) => provider,
        Err(message) => return Report::failed("tui", &Error::invalid_input(message)),
    };
    let mut handler = AgentHandler {
        runtime,
        provider,
        fs: &fs,
        process,
        env,
        model: ModelSpec::new(model),
        max_tokens: args.max_tokens,
        max_steps: args.max_steps,
    };
    let mut app = App::new();
    app.apply_update(Update::Models(models_for(args.provider)));
    match run(app, &mut handler, &clock) {
        Ok(()) => Report::ok("tui", None),
        Err(error) => Report::failed("tui", &Error::io("<tui>", error)),
    }
}

/// Modelos oferecidos no seletor da TUI, por provider (E10-T07).
///
/// Lista estática até o `dynamic_models` (E12-T02) expor o catálogo; o **default** vem primeiro
/// para o índice zero coincidir com o modelo do arranque.
fn models_for(provider: &str) -> Vec<String> {
    let mut models = vec![default_model(provider).to_string()];
    if provider == "opencode-go" {
        models.push("deepseek-v4.1-flash".to_string());
    }
    models
}

/// Executor do loop de turnos para a UI (dono do runtime e do provider).
struct AgentHandler<'a> {
    runtime: Runtime<'a>,
    provider: Box<dyn Provider>,
    fs: &'a StdFs,
    process: StdProcess,
    env: StdEnv,
    model: ModelSpec,
    max_tokens: u32,
    max_steps: u32,
}

impl Handler for AgentHandler<'_> {
    fn handle(&mut self, command: Command, painter: &mut Painter<'_>) -> Vec<Update> {
        match command {
            Command::Quit => Vec::new(),
            Command::Submit(goal) => self.submit(&goal, painter),
            Command::SetModel(model) => {
                self.model.model = model;
                vec![Update::Info(format!("modelo: {}", self.model.model))]
            }
            Command::SetThinking(thinking) => {
                self.model.thinking = thinking;
                vec![Update::Info(format!("pensamento: {thinking:?}"))]
            }
        }
    }
}

impl AgentHandler<'_> {
    /// Submete um turno e traduz o resultado em atualizações da UI.
    fn submit(&mut self, goal: &str, painter: &mut Painter<'_>) -> Vec<Update> {
        let granted_by = self
            .env
            .var("USER")
            .or_else(|| self.env.var("USERNAME"))
            .unwrap_or_else(|| "local".to_string());
        let ports = Ports {
            fs: self.fs,
            process: &self.process,
            env: &self.env,
        };
        let options = TurnOptions {
            model: self.model.clone(),
            system: Some(SYSTEM.to_string()),
            max_tokens: self.max_tokens,
            temperature: 0.0,
            max_steps: self.max_steps,
        };
        let mut activity = LivePainter {
            painter,
            granted_by,
        };
        match run_turn_with(
            &mut self.runtime,
            TurnRequest {
                provider: self.provider.as_ref(),
                ports,
                goal,
                options: &options,
            },
            &mut activity,
        ) {
            Ok(turn) => {
                let mut updates = vec![Update::Assistant(turn.text)];
                if turn.calls > 0 {
                    updates.push(Update::Info(format!("{} tool call(s)", turn.calls)));
                }
                updates.push(Update::Phase(self.runtime.phase().as_str().to_string()));
                updates.push(Update::Done);
                updates
            }
            Err(error) => vec![Update::Error(error.to_string())],
        }
    }
}

/// Adapta o [`Painter`] da UI ao observador efémero do loop (E10-T05) e ao challenge de aprovação
/// (E10-T04).
struct LivePainter<'p, 'a> {
    painter: &'p mut Painter<'a>,
    granted_by: String,
}

impl ActivitySink for LivePainter<'_, '_> {
    fn approve(&mut self, prompt: &ApprovalPrompt<'_>) -> Option<Approval> {
        let request = ChallengePrompt {
            tool: prompt.tool.to_string(),
            rule: prompt.request.rule_id.as_str().to_string(),
            scope: prompt.request.scope.clone(),
        };
        let signature = self.painter.challenge(request, &self.granted_by)?;
        katu_core::event!(
            Level::Warn,
            events::TUI_APPROVAL,
            "tool" => prompt.tool,
            "rule" => prompt.request.rule_id.as_str()
        );
        Some(Approval {
            reason: signature.reason,
            granted_by: signature.granted_by,
        })
    }

    fn activity(&mut self, activity: Activity<'_>) {
        let live = match activity {
            Activity::Text(delta) => {
                katu_core::event!(Level::Trace, events::TUI_LIVE, "kind" => "text");
                Live::Text(delta.to_string())
            }
            Activity::Thinking(delta) => {
                katu_core::event!(Level::Trace, events::TUI_LIVE, "kind" => "thinking");
                Live::Thinking(delta.to_string())
            }
            Activity::Tool { name } => {
                katu_core::event!(Level::Trace, events::TUI_LIVE, "kind" => "tool");
                Live::Tool(name.to_string())
            }
            Activity::ToolDone { name } => {
                katu_core::event!(Level::Trace, events::TUI_LIVE, "kind" => "tool_done");
                Live::ToolDone(name.to_string())
            }
            Activity::Refused { rule, evidence, .. } => {
                katu_core::event!(Level::Warn, events::TUI_LIVE, "kind" => "refused", "rule" => rule);
                Live::Refused {
                    rule: rule.to_string(),
                    evidence: evidence.to_string(),
                }
            }
            Activity::Unavailable { control, .. } => {
                katu_core::event!(Level::Warn, events::TUI_LIVE, "kind" => "unavailable");
                Live::Unavailable {
                    control: control.to_string(),
                }
            }
        };
        self.painter.live(live);
    }
}
