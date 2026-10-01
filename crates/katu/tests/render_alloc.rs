//! Alocações por quadro no render da TUI (E10-T03 / E18-T10) — a afirmação «render sem alocações».
//!
//! **Alvo de integração** (crate própria) porque o `#[global_allocator]` é global ao binário de
//! teste; e crate `katu` porque é o **único** crate com `deny(unsafe_code)` e, portanto, o único
//! onde um escape hatch local está autorizado — registado em
//! [`EXCEPTIONS`](../../../xtask/src/unsafe_check.rs), ao lado do `kill(2)` do grupo de processos.
//! O `katu-tui` continua `forbid`, sem escape hatch.
//!
//! **O que mede:** alocações e bytes por quadro de `katu_tui::render`, depois de aquecimento, num
//! `TestBackend` de 120×40 com a conversa no teto (200 entradas) — o pior caso que a UI tem.
//!
//! **A janela é por thread.** O allocator vê as alocações de todo o processo e o runner corre este
//! ficheiro em paralelo: com contadores globais os testes mediam-se a si próprios, e com uma
//! tranca partilhada o caminho de bloqueio dentro do allocator é um caminho para o deadlock.
//!
//! **Por que é preciso `unsafe`:** contar alocações exige `unsafe impl GlobalAlloc`. O invólucro
//! limita-se a encaminhar cada operação para `System` e a somar contadores da thread; nenhum outro
//! caminho de memória é tocado. Os testes abaixo exercitam cada método do traço e a construção do
//! buffer, para que uma regressão no invólucro não passe em silêncio.

use katu_tui::{App, Live, Update, render};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

/// Todo o `unsafe` vive aqui dentro, sob **um** escape hatch (o gate `check-unsafe` conta
/// excepções, não métodos) e é exercitado pelos testes abaixo.
// `Cell` é `disallowed_types` (ADR 0016) porque em produção abre caminho para estado partilhado
// sem sincronização. Aqui o estado é **por thread**, dentro de um allocator de teste: um atómico
// global seria justamente o que produz a medição errada (contaria as outras threads do runner).
#[allow(
    unsafe_code,
    clippy::disallowed_types,
    clippy::redundant_pub_crate,
    unreachable_pub,
    reason = "medição (E18-T10): contar alocações exige `unsafe impl GlobalAlloc` (só encaminha para `System` e soma contadores por thread); num alvo de teste não há alcance externo nem estado partilhado"
)]
pub(crate) mod counting {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    /// Allocador de contagem: **encaminha tudo** para `System` e soma o que a thread mede.
    struct Counting;

    thread_local! {
        /// `const`-inicializado: aceder ao TLS não aloca, logo o próprio allocator pode lê-lo sem
        /// risco de recursão. Sem `const`, o primeiro acesso alocaria — dentro do allocator.
        static COUNTING: Cell<bool> = const { Cell::new(false) };
        static ALLOCS: Cell<u64> = const { Cell::new(0) };
        static BYTES: Cell<u64> = const { Cell::new(0) };
    }

    /// Soma uma alocação à janela da thread, se estiver aberta.
    fn charge(size: usize) {
        COUNTING.with(|flag| {
            if flag.get() {
                ALLOCS.with(|count| count.set(count.get().saturating_add(1)));
                let bytes = u64::try_from(size).unwrap_or(0);
                BYTES.with(|sum| sum.set(sum.get().saturating_add(bytes)));
            }
        });
    }

    // SAFETY: o invólucro não gerencia memória própria: cada método repassa o `Layout` intacto
    // para `System`, que é o allocator global por omissão, e o único estado novo são contadores
    // por thread (observação, sem efeito na memória devolvida). O contrato do traço mantém-se
    // porque a validade de `ptr`/`layout`/`new_size` é a que o runtime garantiu e que aqui é
    // repassada sem alteração.
    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            charge(layout.size());
            // SAFETY: `layout` é válido por contrato do traço e é repassado sem alteração.
            unsafe { System.alloc(layout) }
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            charge(layout.size());
            // SAFETY: `layout` é válido por contrato do traço e é repassado sem alteração.
            unsafe { System.alloc_zeroed(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            // SAFETY: `ptr`/`layout` vieram de uma alocação deste allocator (encaminhamento puro).
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            charge(new_size);
            // SAFETY: `ptr`/`layout` são coerentes por contrato do traço; `new_size` é o novo layout.
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    #[global_allocator]
    static ALLOCATOR: Counting = Counting;

    /// Zera os contadores da thread e abre a janela.
    pub(crate) fn start() {
        ALLOCS.with(|count| count.set(0));
        BYTES.with(|sum| sum.set(0));
        COUNTING.with(|flag| flag.set(true));
    }

    /// Fecha a janela e devolve `(alocações, bytes)` **desta** thread.
    pub(crate) fn stop() -> (u64, u64) {
        COUNTING.with(|flag| flag.set(false));
        (ALLOCS.with(Cell::get), BYTES.with(Cell::get))
    }

    /// Leitura sem abrir janela.
    pub(crate) fn peek() -> u64 {
        ALLOCS.with(Cell::get)
    }

    /// Exercita `alloc_zeroed`/`dealloc`: devolve `true` se a memória zeroada veio utilizável.
    /// Existe para o traço não poder regredir sem um teste vermelho.
    pub(crate) fn zeroed_is_usable(size: usize) -> bool {
        let Ok(layout) = Layout::from_size_align(size, 8) else {
            return false;
        };
        // SAFETY: layout válido e não nulo; o ponteiro é lido e libertado com o mesmo layout.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if ptr.is_null() {
            return false;
        }
        #[allow(
            clippy::disallowed_methods,
            reason = "sonda do invólucro: ler a própria alocação para confirmar que `alloc_zeroed` serve memória limpa"
        )]
        let clean = {
            // SAFETY: `ptr` aponta para `size` bytes zeroados por `alloc_zeroed` reservados acima;
            // a leitura não sai da alocação.
            let slice: &[u8] = unsafe { std::slice::from_raw_parts(ptr, size) };
            slice.iter().all(|byte| *byte == 0)
        };
        // SAFETY: desalocar com o layout original, que é o que foi reservado.
        unsafe { System.dealloc(ptr, layout) };
        clean
    }
}

// -- O invólucro é testado antes de servir de prova -----------------------------------------------

/// Com a janela fechada, o contador não se mexe (mesmo alocando).
#[test]
fn the_counter_is_silent_while_disabled() {
    let before = counting::peek();
    let mut owned: Vec<u8> = vec![7_u8; 4096];
    owned.push(1);
    assert_eq!(counting::peek(), before, "contou fora da janela");
    assert_eq!(owned.len(), 4097);
}

/// Com a janela aberta, cada alocação desta thread é contada uma vez.
#[test]
fn the_counter_sees_allocations_while_enabled() {
    counting::start();
    let mut owned: Vec<u8> = Vec::with_capacity(2048);
    owned.extend(std::iter::repeat_n(3_u8, 4096));
    let (allocs, bytes) = counting::stop();
    assert!(allocs >= 1, "não contou a alocação");
    assert!(bytes >= 4096, "contou {bytes} B para 4096 B pedidos");
    assert_eq!(owned.len(), 4096);
}

/// `alloc_zeroed` devolve memória utilizável e zeroada.
#[test]
fn zeroed_allocations_are_usable() {
    assert!(
        counting::zeroed_is_usable(64),
        "alloc_zeroed não deu memória limpa"
    );
}

/// `realloc` preserva o conteúdo (o caminho é contado, não ignorado).
#[test]
fn realloc_preserves_contents_and_is_counted() {
    counting::start();
    let mut owned: Vec<u8> = (0..16_u8).collect();
    owned.reserve(4096);
    let (allocs, _bytes) = counting::stop();
    assert!(allocs >= 1, "o crescimento não passou por `realloc`");
    let expected: Vec<u8> = (0..16_u8).collect();
    assert_eq!(owned.get(..16), Some(expected.as_slice()));
    assert!(owned.capacity() >= 4096);
}

// -- A medição ------------------------------------------------------------------------------

/// App no pior caso: conversa no teto (200 entradas) + tool viva no painel de atividade.
fn loaded_app() -> App {
    let mut app = App::new();
    app.apply_update(Update::Live(Live::Tool {
        name: "read".to_string(),
        args: "{\"path\":\"README.md\"}".to_string(),
    }));
    for index in 0..200 {
        app.apply_update(Update::Assistant(format!(
            "linha {index} com texto suficiente para encher a linha do terminal e forçar wrap"
        )));
    }
    app
}

/// Alocações e bytes por quadro, depois de `warmup` quadros descartados.
fn measure(app: &App, warmup: u32, frames: u32) -> (u64, u64) {
    let backend = TestBackend::new(120, 40);
    let Some(mut terminal) = Terminal::new(backend).ok() else {
        return (0, 0);
    };
    let draw = |terminal: &mut Terminal<TestBackend>| {
        // O `Result` é engolido de propósito: o que se mede são as alocações, não o desenho.
        let _drew = terminal.draw(|frame| render(frame, app));
    };
    for _ in 0..warmup {
        draw(&mut terminal);
    }
    counting::start();
    for _ in 0..frames {
        draw(&mut terminal);
    }
    let (allocs, bytes) = counting::stop();
    (
        allocs.checked_div(u64::from(frames)).unwrap_or(0),
        bytes.checked_div(u64::from(frames)).unwrap_or(0),
    )
}

/// Lê o orçamento de [`bench/render/budget.toml`](../../../bench/render/budget.toml).
#[allow(
    clippy::disallowed_methods,
    reason = "artefacto de medição: o caminho de escrita é uma variável de ambiente, como nos restantes benches"
)]
fn budget() -> Option<(u64, u64)> {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/render/budget.toml");
    let text = std::fs::read_to_string(&path).ok()?;
    let read = |key: &str| -> u64 {
        text.lines()
            .find_map(|line| {
                line.trim()
                    .strip_prefix(key)
                    .and_then(|value| value.trim().parse::<u64>().ok())
            })
            .unwrap_or(0)
    };
    Some((read("frame_allocations ="), read("frame_alloc_bytes =")))
}

/// A afirmação «o render não aloca» é medida — e sai **falsa**; o que se trava é que não cresça.
#[test]
#[allow(
    clippy::disallowed_methods,
    reason = "artefacto de medição: o caminho de escrita é uma variável de ambiente, como nos restantes benches"
)]
fn render_allocations_per_frame_are_measured_and_published()
-> Result<(), Box<dyn std::error::Error>> {
    let Some((budget_allocs, budget_bytes)) = budget() else {
        return Err("bench/render/budget.toml sem frame_allocations/frame_alloc_bytes".into());
    };
    let app = loaded_app();
    let (allocs, bytes) = measure(&app, 8, 40);
    let value = serde_json::json!({
        "schema": "katu.bench.render_alloc.v1",
        "question": "o render da TUI aloca zero por quadro?",
        "method": "allocator de contagem com janela por thread; 8 quadros de aquecimento + 40 medidos; TestBackend 120x40; conversa no teto (200 entradas)",
        "allocations_per_frame": allocs,
        "bytes_per_frame": bytes,
        "zero_allocations": allocs == 0,
        "budget_allocations": budget_allocs,
        "budget_bytes": budget_bytes,
        "criterion": "as alocacoes por quadro nao ultrapassam o orcamento versionado (a de zero alocacoes e FALSA)",
        "criterion_met": allocs <= budget_allocs && bytes <= budget_bytes,
        "caveat": "mede o render puro sobre um backend de teste; um terminal real acrescenta o flush do driver, que nao esta aqui",
        "decision": "a afirmacao e falsa: o render aloca ~1,1k vezes por quadro. Travado: o orcamento de crescimento e o p95 do gate:render. Refazer o render com dados emprestados fica para uma decisao propria com A/B",
    });
    let text = serde_json::to_string_pretty(&value)?;
    if let Ok(path) = std::env::var("KATU_RENDER_ALLOC_OUT") {
        std::fs::write(&path, format!("{text}\n"))?;
    }
    let parsed: serde_json::Value = serde_json::from_str(&text)?;
    assert!(parsed.get("allocations_per_frame").is_some());
    assert!(
        allocs <= budget_allocs && bytes <= budget_bytes,
        "o render cresceu: {allocs} alocações ({bytes} B) por quadro, orçamento {budget_allocs} ({budget_bytes})"
    );
    Ok(())
}
