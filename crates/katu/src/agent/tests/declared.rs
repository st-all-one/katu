//! Tool calls **declaradas como texto** (W8-1b): modelos locais sem `tool_calls` nativas.
//!
//! O Qwen2.5-Coder servido por `llama-server` escreve a chamada no `content` (bloco de código JSON
//! ou `<tool_call>`). Sem a decodificação o loop vê zero chamadas, o turno termina e o JSON cru
//! aparece como resposta final. Aqui prova-se o caminho completo: decodifica, executa e o JSON
//! **não** entra no log como mensagem do assistente.

use katu_core::kernel::Message;
use katu_core::ports::{FixedClock, Timestamp};
use katu_core::provider::Provider;
use katu_providers::{FakeProvider, Turn};

use super::{Ports, options, request, root};
use crate::agent::run_turn;
use crate::ports::{StdEnv, StdFs, StdProcess};
use crate::runtime::Runtime;

#[test]
fn a_declared_text_call_is_decoded_and_executed() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("declared-call")?;
    std::fs::write(root.join("nota.txt"), "conteudo da nota\n")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "lê a nota")?;

    let declared =
        "```json\n{\"name\":\"read\",\"arguments\":{\"path\":\"nota.txt\",\"view\":\"full\"}}\n```";
    let provider: std::sync::Arc<dyn Provider> = std::sync::Arc::new(FakeProvider::new(
        "fake",
        vec![Turn::text(declared), Turn::text("li a nota")],
    ));
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };

    let report = run_turn(
        &mut runtime,
        request(&provider, ports, "lê a nota", &options(4)),
    )?;
    assert_eq!(report.calls, 1, "a chamada declarada correu");
    assert_eq!(report.steps, 2);
    assert_eq!(report.text, "li a nota");
    assert!(
        !report.text.contains("\"name\""),
        "o JSON declarado não é resposta final: {}",
        report.text
    );

    let messages = runtime.messages()?;
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::ToolCall { .. }))
    );
    let delta = messages
        .iter()
        .find_map(|message| match message {
            Message::ToolResult { delta, .. } => delta.clone(),
            _ => None,
        })
        .ok_or("o resultado tem de trazer o delta")?;
    assert!(delta.contains("conteudo da nota"), "{delta}");
    // O texto declarado **não** entra no log como mensagem do assistente (senão o modelo veria o
    // próprio JSON no passo seguinte).
    assert!(!messages.iter().any(|message| matches!(
        message,
        Message::Assistant { text } if text.contains("\"name\"")
    )));

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

/// A variante `<tool_call>` (o protocolo do template do Qwen) também é decodificada: o passo não
/// termina no JSON cru e o turno segue até à resposta final.
#[test]
fn a_tagged_declared_call_is_decoded() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("declared-tag")?;
    std::fs::write(root.join("nota.txt"), "conteudo da nota\n")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "lê a nota")?;

    let declared =
        "<tool_call>\n{\"name\":\"read\",\"arguments\":{\"path\":\"nota.txt\"}}\n</tool_call>";
    let provider: std::sync::Arc<dyn Provider> = std::sync::Arc::new(FakeProvider::new(
        "fake",
        vec![Turn::text(declared), Turn::text("pronto")],
    ));
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };

    let report = run_turn(
        &mut runtime,
        request(&provider, ports, "lê a nota", &options(4)),
    )?;
    assert_eq!(report.calls, 1);
    assert_eq!(report.text, "pronto");

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}
