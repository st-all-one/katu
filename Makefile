# katu — alvos de qualidade e build (E01-T03, E15).

CARGO ?= cargo

.PHONY: check fmt clippy test build file-length layers clean \
        deny audit machete typos miri instrument ci

## Portão completo local: formatação, lints, testes, camadas e tamanho de arquivo.
check: fmt clippy test layers file-length

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

## Portão do CI: check + extras disponíveis.
ci: check deny audit machete typos miri
