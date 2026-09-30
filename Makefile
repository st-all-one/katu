# katu — alvos de qualidade e build (E01-T03, E15).

CARGO ?= cargo

.PHONY: check fmt clippy test build file-length layers diag docs policy bench provider measure clean \
        deny audit machete typos miri instrument ci memory-swap

## Portão completo local: formatação, lints, testes, camadas, diag, docs, política e tamanho.
check: fmt clippy test layers diag schemas docs policy memory-swap bench provider file-length

## Verifica formatação sem alterar.
fmt:
	$(CARGO) fmt --all -- --check

## Lints de todos os alvos com warnings como erro.
clippy:
	$(CARGO) clippy --workspace --all-targets -- -D warnings

## Testes do workspace (inclui doc-tests).
test:
	$(CARGO) test --workspace

## Build de debug do workspace.
build:
	$(CARGO) build --workspace

## Nenhum arquivo de produção passa de 300 linhas.
file-length:
	./scripts/check_file_length.sh

## Firewall LLM-free + cobertura de crates.
layers:
	$(CARGO) run -q -p xtask -- check-layers
	$(CARGO) run -q -p xtask -- check-crate-coverage

## Logs só estruturados (DF9/E19): nenhuma macro de texto livre fora do sink.
diag:
	$(CARGO) run -q -p xtask -- check-diag

## Schema das tools válido (E06-T02): nomes, descrições, enums e anti-poisoning.
schemas:
	$(CARGO) run -q -p xtask -- check-schemas

## Documentação: todos os links de `*.md` resolvem (E01-T05).
docs:
	$(CARGO) run -q -p xtask -- check-docs

## Política: auditoria de regras (E02-T04) + ledger de cobertura (E02-T06).
policy:
	$(CARGO) run -q -p xtask -- policy:audit
	$(CARGO) run -q -p xtask -- ledger:validate

## Números publicados: nenhum valor sem base e artefacto (DF5/E15-T02).
bench:
	$(CARGO) run -q -p xtask -- gate:bench

## Latência do provider: mede o overhead de cliente e trava contra o orçamento (E12-T07/E18-T04).
provider:
	$(CARGO) run -q -p xtask -- gate:provider

## Substituibilidade da memória (E03-T06): o knudge só acopla no adaptador; o binário também
## compila sem o adaptador (memória é invariante em produção, G4/E03-T07).
memory-swap:
	$(CARGO) run -q -p xtask -- check-memory-swap
	$(CARGO) check -p katu --no-default-features

## Medição do MVK (E05-T06): gera o artefacto cru e valida o manifesto.
measure:
	$(CARGO) run -q -p katu --features profile --example measure_mvk
	$(CARGO) run -q -p xtask -- gate:bench

clean:
	$(CARGO) clean

# --- Alvos extras (CI / verificação). Pulam se a ferramenta não estiver instalada. ---

## Supply chain: licenças/advisories/fontes.
deny:
	@if command -v cargo-deny >/dev/null 2>&1; then \
		$(CARGO) deny check; \
	else \
		echo "cargo-deny ausente; pule (cargo install cargo-deny --locked)"; \
	fi

audit:
	@if command -v cargo-audit >/dev/null 2>&1; then \
		$(CARGO) audit; \
	else \
		echo "cargo-audit ausente; pule (cargo install cargo-audit --locked)"; \
	fi

## Dependências não usadas.
machete:
	@if command -v cargo-machete >/dev/null 2>&1; then \
		$(CARGO) machete; \
	else \
		echo "cargo-machete ausente; pule (cargo install cargo-machete --locked)"; \
	fi

## Verificação ortográfica.
typos:
	@if command -v typos >/dev/null 2>&1; then \
		typos; \
	else \
		echo "typos ausente; pule (https://github.com/crate-ci/typos)"; \
	fi

## Verificação dinâmica de UB (crates puros). Miri exige nightly.
miri:
	@if command -v cargo-miri >/dev/null 2>&1 && $(CARGO) +nightly miri --version >/dev/null 2>&1; then \
		PROPTEST_DISABLE_FAILURE_PERSISTENCE=1 \
		MIRIFLAGS="$${MIRIFLAGS:-} -Zmiri-disable-isolation" \
		$(CARGO) +nightly miri test -p katu-policy -p katu-core; \
	else \
		echo "miri ausente; pule (rustup +nightly component add miri)"; \
	fi

## Instrumentação transversal (DF9/E19): compila e linta com a feature ligada.
instrument:
	$(CARGO) clippy -p katu-core --features instrument --all-targets -- -D warnings
	$(CARGO) clippy -p katu --features profile --all-targets -- -D warnings
	$(CARGO) test -p katu-core --features instrument
	$(CARGO) test -p katu --features profile

## Portão do CI: check + extras disponíveis.
ci: check deny audit machete typos miri
