//! Testes de render da UI (E10) num backend de teste (sem terminal real).

use katu_tui::{Action, App, Live, TrashEntry, Update, render};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

/// Renderiza num backend de teste e devolve o texto do buffer.
fn draw(app: &App) -> Result<String, Box<dyn std::error::Error>> {
    draw_sized(app, 60, 16)
}

/// Renderiza num backend de teste com um tamanho explícito.
fn draw_sized(app: &App, width: u16, height: u16) -> Result<String, Box<dyn std::error::Error>> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|frame| render(frame, app))?;
    let buffer = terminal.backend().buffer();
    Ok(buffer
        .content()
        .iter()
        .map(|cell| cell.symbol().to_string())
        .collect())
}

#[test]
fn renders_header_and_transcript() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    app.apply_update(Update::Assistant("olá mundo".to_string()));
    let text = draw(&app)?;
    assert!(text.contains("katu"), "{text}");
    assert!(text.contains("olá mundo"), "{text}");
    Ok(())
}

#[test]
fn renders_model_and_thinking_in_the_header() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    app.apply_update(Update::Models(vec!["qwen".to_string()]));
    app.apply_action(Action::CycleThinking);
    let text = draw(&app)?;
    assert!(text.contains("qwen"), "{text}");
    assert!(text.contains("pensamento low"), "{text}");
    Ok(())
}

#[test]
fn renders_the_trash_overlay() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    app.apply_action(Action::OpenTrash);
    app.apply_update(Update::Trash(vec![TrashEntry {
        original: "src/a.rs".to_string(),
        stored: "/t/a".to_string(),
    }]));
    let text = draw(&app)?;
    assert!(text.contains("lixeira"), "{text}");
    assert!(text.contains("src/a.rs"), "{text}");
    Ok(())
}

#[test]
fn renders_the_transcript_viewer() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    app.apply_action(Action::OpenTranscript);
    app.apply_update(Update::Transcript(vec![
        "# katu — transcrição".to_string(),
        "**utilizador**".to_string(),
    ]));
    let text = draw(&app)?;
    assert!(text.contains("transcrição"), "{text}");
    assert!(text.contains("utilizador"), "{text}");
    Ok(())
}

#[test]
fn renders_the_declared_next_action() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    app.apply_update(Update::NextAction("verificar".to_string()));
    let text = draw_sized(&app, 120, 16)?;
    assert!(text.contains("próximo verificar"), "{text}");
    Ok(())
}

#[test]
fn renders_phase_from_state() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    app.apply_update(Update::Phase("verified".to_string()));
    let text = draw(&app)?;
    assert!(text.contains("verified"), "{text}");
    Ok(())
}

#[test]
fn renders_only_the_tail_of_a_large_transcript() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    for index in 0..240 {
        app.apply_update(Update::Info(format!("linha {index}")));
    }
    let text = draw(&app)?;
    assert!(text.contains("linha 239"), "mostra o fim: {text}");
    assert!(!text.contains("linha 0 "), "não mostra o início: {text}");
    Ok(())
}

#[test]
fn renders_live_activity_panel() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    app.apply_update(Update::Live(Live::Tool("grep".to_string())));
    app.apply_update(Update::Live(Live::Text("a responder".to_string())));
    let text = draw(&app)?;
    assert!(text.contains("atividade"), "{text}");
    assert!(text.contains("grep"), "{text}");
    assert!(text.contains("a responder"), "{text}");
    Ok(())
}
