//! Testes da secção `estado` no prime (Q-04) e da política de seleção (Q-02b/Q-03).

use katu_core::context::{MAX_SECTION_BYTES, SelectionPolicy};
use katu_core::kernel::{Event, read_records};
use katu_core::ports::{FixedClock, Timestamp};

use super::Runtime;
use crate::ports::StdFs;

/// Runtime recém-aberto sobre uma raiz temporária.
fn runtime<'a>(
    fs: &'a StdFs,
    clock: &'a FixedClock,
    label: &str,
) -> Result<Runtime<'a>, Box<dyn std::error::Error>> {
    let root = super::root(label)?;
    Ok(Runtime::open(fs, clock, &root, "teste")?)
}

#[test]
fn the_prompt_state_is_off_by_default_and_the_prime_stays_static()
-> Result<(), Box<dyn std::error::Error>> {
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = runtime(&fs, &clock, "state-off")?;
    assert!(!runtime.prompt_state(), "Q-04 exige A/B: fica desligado");
    runtime.record_prompt_state(4)?;
    assert!(
        runtime.state_text().is_none(),
        "desligado não registra nada"
    );
    let context = runtime.context()?;
    assert!(
        !context.prime.contains("estado:"),
        "o prime estático não muda: {}",
        context.prime
    );
    drop(runtime);
    std::fs::remove_dir_all(super::root("state-off")?)?;
    Ok(())
}

#[test]
fn the_prompt_state_is_logged_and_enters_the_prime() -> Result<(), Box<dyn std::error::Error>> {
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = runtime(&fs, &clock, "state-on")?;
    runtime.set_prompt_state(true);
    runtime.record_user("olá")?;
    runtime.record_prompt_state(6)?;

    let text = runtime
        .state_text()
        .ok_or("ligada registra a secção")?
        .to_string();
    assert!(text.len() <= MAX_SECTION_BYTES);
    assert!(text.contains("modo execucao"), "{text}");
    assert!(text.contains("passos 6"), "{text}");

    // O prime leva a secção exatamente como foi registada (fim do prompt).
    let context = runtime.context()?;
    assert!(context.prime.ends_with(&text), "{}", context.prime);

    // `Model-visible ⟺ logged`: o log traz o mesmo texto, no turno corrente.
    let path = runtime.session().log_path().to_path_buf();
    let logged = read_records(&fs, &path)?
        .into_iter()
        .find_map(|record| match record.event {
            Event::PromptState { text, .. } => Some(text),
            _ => None,
        });
    assert_eq!(logged.as_deref(), Some(text.as_str()));
    drop(runtime);
    std::fs::remove_dir_all(super::root("state-on")?)?;
    Ok(())
}

#[test]
fn the_selection_policy_reaches_the_context() -> Result<(), Box<dyn std::error::Error>> {
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = runtime(&fs, &clock, "selection")?;
    assert_eq!(
        runtime.selection(),
        SelectionPolicy::Suffix,
        "default histórico"
    );
    runtime.set_selection(SelectionPolicy::Utility);
    assert_eq!(runtime.selection(), SelectionPolicy::Utility);
    let options = runtime.assemble_options();
    assert_eq!(options.selection, SelectionPolicy::Utility);
    assert_eq!(
        options.goal, "teste",
        "o objetivo alimenta o canal `objetivo`"
    );
    drop(runtime);
    std::fs::remove_dir_all(super::root("selection")?)?;
    Ok(())
}
