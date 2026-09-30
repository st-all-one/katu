# E13 — Testes e qualidade (transversal)

> **Transversal a todas as fases.** As regras do projeto sobre como provar que algo funciona.
> É a cura direta das cicatrizes do `open-keyboard` (§26) e do `maxima` (§49.6).
>
> **Decisões:** DF1, DF2, DF5, DF7. **Depende de:** todas.
> **Gate permanente:** um guard só guarda se a regressão o falhar.

---

## As cinco regras herdadas do `dsh` (§44)

1. **Verificar o mundo, não o auto-relato.** Um e2e re-executa o comando ou relê o ficheiro
   externamente; uma sonda de palavras-chave na própria saída do agente deixa um agente trapeceiro
   passar. Assertar que ficheiros **não** tocados ficaram byte-idênticos.
2. **Testar o caminho de entrada real.** Um guard só guarda se a regressão o falhar. Para provar,
   **introduzir a regressão, ver vermelho, reverter** (§51.9).
3. **Preferir a implementação real ao mock.** Mockar só a fronteira cara ou não-determinística
   (provider LLM, rede, relógio).
4. **Especs correm concorrentemente.** Uma spec que só passa quando corre sozinha é defeito da spec.
5. **Cada spec possui os seus recursos** até ao teardown, mesmo em falha/retry/timeout.

---

## Tarefas

### E13-T01 ◐ Camadas de teste
- **Entregáveis:** unit (por crate), integração (loop real), e2e (binário, subprocesso), golden,
  property (`proptest`), fuzz (`cargo-fuzz` para parsers de bytes).
- **Estado:** alvos `cargo xtask test:{unit,integration,e2e,all}` (`xtask/src/test_runner.rs`); a
  integração descobre `crates/*/tests/*.rs` e o e2e do binário corre como subprocesso (`-p katu
  --test cli`); golden já existe (`crates/katu-policy/tests/golden.rs`). **Property:** `proptest`
  na emissão TOON (`katu-core`) e na leitura defensiva de modelos (`parse_models`, `katu-providers`,
  totalidade + determinismo). O CI separa rápido (PR) de pesado (merge). **Falta:** fuzz
  (`cargo-fuzz`) para parsers de bytes.
- **Aceite:** cada camada tem um alvo `cargo xtask test:<level>`; o CI corre rápido (unit+lint) no
  PR e pesado (e2e+bench) no merge.

### E13-T02 ☑ Regra "testar o caminho real por regra de política"
- **Entregáveis:** para cada regra `Enforced`, um teste de integração que conduz o loop e asserta a
  negação com evidência.
- **Estado:** `coverage.toml` (versionado) mapeia cada regra `Enforced` ao teste que a cobre;
  `xtask check-rule-coverage` (`xtask/src/coverage.rs`) verifica a **totalidade** (nenhuma regra sem
  teste, nenhuma entrada órfã) e que o teste nomeado existe nas fontes. Integrado em
  `cargo xtask check`.
- **Aceite:** a matriz regra ↔ teste é **total** (nenhuma regra sem teste de caminho real); o
  ledger de cobertura (E02-T06) é consultado.

### E13-T03 ☐ Teste de regressão invertido (mutation-like)
- **Entregáveis:** um harness `xtask test:guard <rule_id>` que remove o enforcement, corre o teste
  e exige vermelho; reverte.
- **Aceite:** todos os guards passam o teste invertido; qualquer guard que não fique vermelho é
  reportado como falso guard.

### E13-T04 ☑ Miri, geiger e Machete no CI
- **Entregáveis:** `cargo miri test` nos crates puros; `cargo geiger` (unsafe confinado);
  `cargo machete`/`udeps` (deps mortas — a lição do `open-keyboard`, §26.4).
- **Estado:** CI com o job `miri` (`cargo +nightly miri test -p katu-core -p katu-policy`) e o job
  `hygiene` (`check-unsafe` + `cargo machete` + `cargo geiger`). `xtask check-unsafe`
  (`xtask/src/unsafe_check.rs`) confirma `#![forbid(unsafe_code)]` na raiz de cada crate puro e
  recusa `allow(unsafe_code)`; `cargo machete` limpo.
- **Aceite:** Miri verde; zero `unsafe` fora do sandbox; zero dependência declarada e não usada.

### E13-T05 ☑ Goldens e fixtures regeneráveis
- **Entregáveis:** dados de referência com regeneração explícita (`KATU_GEN_TEST_DATA=1`); PR que
  toca dados de referência exige **dupla revisão** (§36).
- **Estado:** `crates/katu-policy/tests/golden/verdicts.tsv` (artefacto golden) comparado pelo teste
  `golden_matrix`; `KATU_GEN_TEST_DATA=1 cargo test -p katu-policy --test golden` regenera. Os
  veredictos inline continuam a assertar (o artefacto é secundário).
- **Aceite:** um snapshot refresh **nunca** é tratado como revisão de correção (postmortem 0002).

### E13-T06 ☐ Matriz de aceitação por tool/verbo
- **Entregáveis:** matriz que liga cada tool/subcomando a `stdout`/`--json`/erro/exit/estado.
- **Aceite:** a matriz é total; um novo verbo sem entrada falha o `xtask`.

### E13-T07 ☑ Invariantes de runtime testadas
- **Entregáveis:** `Model-visible ⟺ logged`; "nunca `Ok` com erros"; "sem passthrough silencioso";
  "escrita atómica".
- **Estado:** `crates/katu-core/src/kernel/session/tests/invariants.rs` — `model_visible_is_exactly_
  the_logged_messages` (histórico = projeção do log; `Session::verify`) e `refused_event_leaves_
  state_and_log_intact` (escrita atómica). Já existiam testes de passthrough (budget/deny).
- **Aceite:** cada invariante tem teste que a viola por um caminho alternativo e falha.

---

## Definition of Done (permanente)

- [ ] `cargo xtask check` + `test:all` verdes.
- [ ] Matriz regra ↔ teste total; guards passam o teste invertido.
- [ ] Miri/geiger/machete verdes.
- [ ] Nenhum arquivo de produção > 300 linhas; zero `unwrap/expect/panic` em `src/`.
- [ ] job `msrv` verde em Rust 1.97.0.

## Anti-checklist (o que **não** fazer)

- Não medir cobertura como prova de que a funcionalidade funciona (§44).
- Não corrigir um incumprimento só melhorando o prompt (§52.13).
- Não publicar benchmark que não compila nem número sem artefacto (§26, §49).
- Não deixar documento e código divergirem num número (hard caps §49.10).
