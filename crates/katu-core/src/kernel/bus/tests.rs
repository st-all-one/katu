use super::{EventBus, HandlerError, HandlerResult, Middleware, Next, Observer};
use crate::error::lock_recover;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Default)]
struct Counter {
    events: AtomicUsize,
}

impl Observer<u32> for Counter {
    fn on_event(&self, _event: &u32) {
        self.events.fetch_add(1, Ordering::SeqCst);
    }
}

/// Regista uma etiqueta quando o método corre, para verificar a ordem.
#[derive(Default)]
struct Trace {
    tags: Mutex<Vec<&'static str>>,
}

impl Trace {
    fn record(&self, tag: &'static str) {
        lock_recover(&self.tags).push(tag);
    }

    fn snapshot(&self) -> Vec<&'static str> {
        lock_recover(&self.tags).clone()
    }
}

/// Middleware que só observa e **não** chama `next` (o bug que o bus tem de detetar).
struct Silent;

impl Middleware<u32> for Silent {
    fn name(&self) -> &'static str {
        "silent"
    }

    fn around(&self, _event: &u32, _next: &mut Next<'_, u32>) -> HandlerResult {
        Ok(())
    }
}

/// Middleware que chama `next` duas vezes.
struct Twice;

impl Middleware<u32> for Twice {
    fn name(&self) -> &'static str {
        "twice"
    }

    fn around(&self, event: &u32, next: &mut Next<'_, u32>) -> HandlerResult {
        next.run(event)?;
        next.run(event)
    }
}

/// Guarda: corta o fluxo devolvendo erro **sem** chamar `next`.
struct Guard;

impl Middleware<u32> for Guard {
    fn name(&self) -> &'static str {
        "guard"
    }

    fn around(&self, _event: &u32, _next: &mut Next<'_, u32>) -> HandlerResult {
        Err(HandlerError::Failed {
            name: "guard",
            message: "recusado".to_string(),
        })
    }
}

/// Middleware que anota a passagem e continua.
struct Tagger<'a> {
    trace: &'a Trace,
    tag: &'static str,
}

impl Middleware<u32> for Tagger<'_> {
    fn name(&self) -> &'static str {
        self.tag
    }

    fn around(&self, event: &u32, next: &mut Next<'_, u32>) -> HandlerResult {
        self.trace.record(self.tag);
        next.run(event)
    }
}

#[test]
fn emit_notifies_every_observer() {
    let first = Counter::default();
    let second = Counter::default();
    let terminal = |_event: &u32| Ok(());
    let bus = EventBus::new(&terminal)
        .with_observer(&first)
        .with_observer(&second);
    bus.emit(&1);
    bus.emit(&2);
    assert_eq!(first.events.load(Ordering::SeqCst), 2);
    assert_eq!(second.events.load(Ordering::SeqCst), 2);
}

#[test]
fn middleware_runs_in_registration_order() {
    let trace = Trace::default();
    let first = Tagger {
        trace: &trace,
        tag: "a",
    };
    let second = Tagger {
        trace: &trace,
        tag: "b",
    };
    let terminal = |event: &u32| {
        assert_eq!(*event, 7);
        Ok(())
    };
    let bus = EventBus::new(&terminal)
        .with_middleware(&first)
        .with_middleware(&second);
    assert_eq!(bus.dispatch(&7), Ok(()));
    assert_eq!(trace.snapshot(), vec!["a", "b"]);
}

#[test]
fn silent_middleware_is_detected() {
    let terminal = |_event: &u32| Ok(());
    let bus = EventBus::new(&terminal).with_middleware(&Silent);
    assert_eq!(
        bus.dispatch(&0),
        Err(HandlerError::SkippedNext { name: "silent" })
    );
}

#[test]
fn calling_next_twice_fails() {
    let terminal = |_event: &u32| Ok(());
    let bus = EventBus::new(&terminal).with_middleware(&Twice);
    assert_eq!(bus.dispatch(&0), Err(HandlerError::CalledTwice));
}

#[test]
fn guard_short_circuits_without_skipped_next() {
    let counter = Counter::default();
    let terminal = |_event: &u32| Ok(());
    let bus = EventBus::new(&terminal)
        .with_observer(&counter)
        .with_middleware(&Guard);
    assert_eq!(
        bus.dispatch(&0),
        Err(HandlerError::Failed {
            name: "guard",
            message: "recusado".to_string(),
        })
    );
    // O observador corre mesmo quando a cadeia corta.
    assert_eq!(counter.events.load(Ordering::SeqCst), 1);
}
