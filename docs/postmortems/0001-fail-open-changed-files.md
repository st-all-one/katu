# 0001 — `changed_files` absoluto faz o gate de verificação falhar aberto

## Executive summary

O gate de verificação (E09-T03) comparava os ficheiros alterados com o **escopo** declarado. Os
caminhos eram guardados em forma absoluta, mas o escopo é relativo ao projeto; a comparação nunca
casava e o gate **passava sempre** — uma falha aberta silenciosa.

## Impact

Uma verificação que devia bloquear passava com zero cobertura efetiva. O utilizador via "verificado"
sem que a alteração tivesse sido confrontada com o escopo. Sistémico: qualquer alteração, em
qualquer projeto.

## Timeline

- Deteção: revisão do caminho de verificação durante E09-T03.
- Correção: normalizar `changed_files` para **relativo** à raiz e comparar com o escopo.

## Root cause

O `Session` registava os caminhos como o `Fs` os devolvia (absolutos), enquanto o escopo é
declarado relativo. Não havia teste que afirmasse a **falha** do gate — só o caminho felizes.

## Guardrails added

- Caminhos relativos em `Session::changed_files` e comparação com o escopo no gate.
- Testes que exigem **bloqueio** quando a alteração sai do escopo:
  [`verify`](../../crates/katu/src/agent/tests/verify.rs) e
  [`runtime`](../../crates/katu/src/runtime/verify.rs).

## Lessons

Um gate sem teste de falha é um gate que pode estar sempre verde. "Fail-closed" exige provar a
negação, não só a permissão.
