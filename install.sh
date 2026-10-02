#!/usr/bin/env bash
# katu installer — estilo `curl | bash` (release) ou `./install.sh --from-source`.
#
# Modo source: com `--from-source` (ou rodando `./install.sh` de dentro do repositório), faz
# `cargo build --release -p katu` e instala o binário otimizado. É o modo fiável: o katu não
# publica binários pré-compilados por omissão.
#
# Modo release: baixa o binário pré-compilado do GitHub Release (latest por padrão, ou
# VERSION=vX.Y.Z), verifica o checksum SHA-256 e instala em ~/.local/bin.
#
# O pacote contém um binário:
#   katu        → CLI + TUI (agente de código com loop possuído, política e memória)
#
# A memória de primeira classe (feature `memory-in-process`, ligada por omissão) exige o
# submódulo `crates/knudge`. Se ele estiver ausente, o instalador tenta inicializá-lo; se não
# conseguir, compila sem memória (`--no-default-features`) e avisa — nunca falha em silêncio.
#
# Nada é apagado de forma irreversível: artefatos antigos são **movidos** para um lixo
# recuperável em ${XDG_CACHE_HOME:-~/.cache}/katu/trash (invalida o caminho original).
#
# Uso:
#   ./install.sh --from-source
#   ./install.sh --from-source --install-dir /usr/local/bin
#   ./install.sh --from-source --profile            # instrumentação transversal (DF9/E19)
#   ./install.sh --from-source --no-memory          # sem o submódulo knudge
#   ./install.sh --uninstall
#   curl ... | VERSION=v0.1.0 bash                  # modo release (quando houver releases)
#
# Variáveis de ambiente:
#   VERSION             tag a instalar (default: latest)
#   INSTALL_DIR         diretório de destino (default: ~/.local/bin)
#   CARGO               binário do cargo (default: cargo)
#   KATU_TARGET         sobrescreve o target triple detectado
#   KATU_REPO           owner/repo do GitHub (default: st-all-one/katu)
#   KATU_BASE_URL       base URL para download (default: https://github.com)
#   KATU_CACHE_DIR      raiz de cache/lixo (default: ${XDG_CACHE_HOME:-~/.cache}/katu)
#   KATU_NO_PATH=1      não edita os rc files (PATH)
#   KATU_NO_LOCKED=1    não passa `--locked` ao cargo
set -euo pipefail

if [ -z "${BASH_VERSION:-}" ]; then
    printf '%s\n' "Este instalador usa recursos do bash. Rode: bash install.sh" >&2
    exit 1
fi

REPO="${KATU_REPO:-st-all-one/katu}"
BASE_URL_ROOT="${KATU_BASE_URL:-https://github.com}"
CACHE_ROOT="${KATU_CACHE_DIR:-${XDG_CACHE_HOME:-${HOME}/.cache}/katu}"
BINARIES=(katu)
INSTALL_DIR="${INSTALL_DIR:-${HOME}/.local/bin}"
VERSION="${VERSION:-latest}"
CARGO="${CARGO:-cargo}"
FROM_SOURCE=0
DO_UNINSTALL=0
NO_MEMORY=0
PROFILE=0
LOCKED="${KATU_NO_LOCKED:+0}"; LOCKED="${LOCKED:-1}"

# HTTPS obrigatório por padrão (evita downgrade de TLS no `curl | bash`). Um `KATU_BASE_URL`
# `http://` explícito (servidor de teste local) libera — nunca é o caminho default.
CURL_PROTO=""
case "$BASE_URL_ROOT" in
    https://*) CURL_PROTO="--proto =https --proto-redir =https --tlsv1.2" ;;
esac

# ── helpers ──────────────────────────────────────────────────────────────────

info() { printf '\033[34m==>\033[0m %s\n' "$*"; }
ok()   { printf '\033[32m  ✓\033[0m %s\n' "$*"; }
warn() { printf '\033[33m  !\033[0m %s\n' "$*" >&2; }
err()  { printf '\033[31m  ✗\033[0m %s\n' "$*" >&2; exit 1; }

require() {
    command -v "$1" >/dev/null 2>&1 || err "Ferramenta necessária não encontrada: $1"
}

# Caminho com $HOME abreviado para ~ (só na mensagem).
tilde() { printf '%s' "${1/#$HOME/\~}"; }

# Move um caminho para o lixo recuperável (nunca apaga; invalida o caminho original).
trash() {
    local src="$1"
    [ -e "$src" ] || [ -L "$src" ] || return 0
    local dest="${CACHE_ROOT}/trash/$(basename "$src").$(date +%Y%m%d%H%M%S).$$"
    mkdir -p "${CACHE_ROOT}/trash"
    mv -- "$src" "$dest"
}

usage() {
    cat <<'EOF'
katu installer

Uso: install.sh [--from-source] [--install-dir DIR] [--version TAG] [--uninstall]

  --from-source, -s     compila a partir do repositório (cargo build --release -p katu)
  --profile             compila com a feature `profile` (instrumentação transversal, DF9/E19)
  --no-memory           compila sem a memória in-process (--no-default-features; experimental)
  --install-dir DIR     diretório de destino (default: ~/.local/bin)
  --prefix DIR          equivale a --install-dir DIR/bin
  --version TAG         tag a instalar no modo release (default: latest)
  --no-path             não edita os rc files para adicionar o PATH
  --uninstall           move o binário instalado para o lixo recuperável
  -h, --help            mostra esta ajuda
EOF
}

# ── argumentos ───────────────────────────────────────────────────────────────

while [ $# -gt 0 ]; do
    case "$1" in
        --from-source|-s) FROM_SOURCE=1 ;;
        --uninstall)      DO_UNINSTALL=1 ;;
        --profile)        PROFILE=1 ;;
        --no-memory)      NO_MEMORY=1 ;;
        --install-dir)    [ $# -ge 2 ] || err "--install-dir exige um valor"; INSTALL_DIR="$2"; shift ;;
        --install-dir=*)  INSTALL_DIR="${1#*=}" ;;
        --prefix)         [ $# -ge 2 ] || err "--prefix exige um valor"; INSTALL_DIR="$2/bin"; shift ;;
        --prefix=*)       INSTALL_DIR="${1#*=}/bin" ;;
        --version)        [ $# -ge 2 ] || err "--version exige um valor"; VERSION="$2"; shift ;;
        --version=*)      VERSION="${1#*=}" ;;
        --no-path)        KATU_NO_PATH=1 ;;
        -h|--help)        usage; exit 0 ;;
        *) err "Argumento desconhecido: $1 (use --help)" ;;
    esac
    shift
done

# Diretório do script (quando é um arquivo real, não `curl | bash`).
SCRIPT_DIR="$(pwd)"
if [ -n "${BASH_SOURCE[0]:-}" ] && [ -f "${BASH_SOURCE[0]}" ]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    if [ "$FROM_SOURCE" -eq 0 ] && [ "$DO_UNINSTALL" -eq 0 ] \
        && [ -f "${SCRIPT_DIR}/Cargo.toml" ] && [ -f "${SCRIPT_DIR}/crates/katu/Cargo.toml" ]; then
        FROM_SOURCE=1
        info "Repositório detectado; instalando a partir do source."
    fi
fi

# ── detecção de OS/arquitetura → target triple ──────────────────────────────

detect_target() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Linux)  os="linux" ;;
        Darwin) os="darwin" ;;
        MINGW*|MSYS*|CYGWIN*) os="windows" ;;
        *) err "Sistema operacional não suportado: $os" ;;
    esac

    case "$arch" in
        x86_64|amd64)  arch="x86_64" ;;
        aarch64|arm64) arch="aarch64" ;;
        *) err "Arquitetura não suportada: $arch" ;;
    esac

    case "$os-$arch" in
        linux-x86_64)   echo "x86_64-unknown-linux-musl" ;;
        linux-aarch64)  echo "aarch64-unknown-linux-musl" ;;
        darwin-aarch64) echo "aarch64-apple-darwin" ;;
        darwin-x86_64)  echo "x86_64-apple-darwin" ;;
        windows-x86_64) echo "x86_64-pc-windows-msvc" ;;
        windows-aarch64) echo "aarch64-pc-windows-msvc" ;;
        *) err "Sem binário publicado para $os/$arch" ;;
    esac
}

# ── resolução da versão ──────────────────────────────────────────────────────

resolve_version() {
    if [ "$VERSION" != "latest" ]; then
        echo "$VERSION"
        return
    fi
    # Segue o redirect de /releases/latest e extrai a tag do caminho final.
    local effective tag
    # `$CURL_PROTO` é intencionalmente não-entre-aspas (word-split das flags).
    # shellcheck disable=SC2086
    effective="$(curl -fsSL $CURL_PROTO -I -o /dev/null -w '%{url_effective}' "${BASE_URL_ROOT}/${REPO}/releases/latest")" \
        || err "Falha ao resolver a versão latest em ${BASE_URL_ROOT}/${REPO} (use --from-source)"
    tag="${effective##*/tag/}"
    case "$tag" in
        v[0-9]*|[0-9]*) echo "$tag" ;;
        *) err "Não foi possível resolver a versão latest (release não encontrado em ${BASE_URL_ROOT}/${REPO}; use --from-source)" ;;
    esac
}

# ── submódulo knudge (memória de primeira classe) ────────────────────────────

# Garante `crates/knudge/crates/knudge-core`. Sem ele, a feature por omissão `memory-in-process`
# não compila; falha com instruções em vez de produzir um binário capenga.
ensure_submodule() {
    [ "$NO_MEMORY" -eq 1 ] && return 0
    local core="${SCRIPT_DIR}/crates/knudge/crates/knudge-core/Cargo.toml"
    [ -f "$core" ] && return 0

    if [ -f "${SCRIPT_DIR}/.gitmodules" ] && command -v git >/dev/null 2>&1; then
        info "Inicializando o submódulo knudge (memória de primeira classe)..."
        if ( cd "$SCRIPT_DIR" && git submodule update --init --recursive ); then
            [ -f "$core" ] && { ok "Submódulo knudge pronto."; return 0; }
        fi
        warn "Não foi possível inicializar o submódulo knudge."
    fi

    err "knudge-core ausente (submódulo crates/knudge). Rode: git submodule update --init --recursive — ou compile sem memória com --no-memory (experimental)."
}

# ── build a partir do source ─────────────────────────────────────────────────

build_from_source() {
    require "$CARGO"
    [ -f "${SCRIPT_DIR}/Cargo.toml" ] || err "--from-source exige rodar de dentro do repositório (Cargo.toml não encontrado em ${SCRIPT_DIR})"
    ensure_submodule

    local args=(build --release)
    [ "$LOCKED" = "1" ] && args+=(--locked)
    args+=(-p katu)
    [ "$NO_MEMORY" -eq 1 ] && args+=(--no-default-features)

    local feats=()
    [ "$PROFILE" -eq 1 ] && feats+=(profile)
    if [ "${#feats[@]}" -gt 0 ]; then
        args+=(--features "$(IFS=,; printf '%s' "${feats[*]}")")
    fi

    info "Compilando release a partir do source: cargo ${args[*]}"
    ( cd "$SCRIPT_DIR" && "$CARGO" "${args[@]}" )
    SRC_DIR="${SCRIPT_DIR}/target/release"
}

# ── download do release ──────────────────────────────────────────────────────

download_release() {
    local target="$1"
    local ext="tar.gz"
    if [ "$target" = *-pc-windows-* ]; then
        ext="zip"
        require unzip
    fi

    local version_no_v="${VERSION#v}"
    local asset="katu-${version_no_v}-${target}.${ext}"
    local base_url="${BASE_URL_ROOT}/${REPO}/releases/download/${VERSION}"

    # Staging fixo em cache (sobrevive entre execuções; nada é apagado).
    local staging="${CACHE_ROOT}/staging"
    mkdir -p "$staging"
    # Invalida o staging anterior movendo-o para o lixo.
    trash "${staging}/out"
    mkdir -p "${staging}/out"

    info "Baixando ${asset}..."
    # shellcheck disable=SC2086
    curl -fsSL $CURL_PROTO -o "${staging}/${asset}" "${base_url}/${asset}" \
        || err "Falha no download: ${base_url}/${asset} (use --from-source)"

    info "Verificando checksum SHA-256..."
    if ! command -v sha256sum >/dev/null 2>&1 && ! command -v shasum >/dev/null 2>&1; then
        err "Nenhum verificador SHA-256 encontrado (instale coreutils ou perl-Digest-SHA)"
    fi
    # shellcheck disable=SC2086
    curl -fsSL $CURL_PROTO -o "${staging}/sha256sums.txt" "${base_url}/sha256sums.txt" \
        || err "Falha no download do checksum: ${base_url}/sha256sums.txt"
    (
        cd "$staging"
        grep -F "  ${asset}" sha256sums.txt > "checksum.one" \
            || err "Checksum para ${asset} não encontrado em sha256sums.txt"
        sha256sum -c "checksum.one" >/dev/null 2>&1 \
            || shasum -a 256 -c "checksum.one" >/dev/null 2>&1 \
            || err "Verificação de checksum falhou. Abortando por segurança."
    )
    ok "Checksum OK"

    info "Extraindo binário..."
    if [ "$ext" = "zip" ]; then
        unzip -o -q "${staging}/${asset}" -d "${staging}/out"
    else
        tar --no-same-owner -xzf "${staging}/${asset}" -C "${staging}/out"
    fi
    SRC_DIR="${staging}/out"
}

# ── instalação ───────────────────────────────────────────────────────────────

install_binaries() {
    mkdir -p "$INSTALL_DIR" 2>/dev/null || err "Não foi possível criar $(tilde "$INSTALL_DIR") (use sudo ou outro --install-dir)"
    [ -w "$INSTALL_DIR" ] || err "$(tilde "$INSTALL_DIR") não é gravável (use sudo ou outro --install-dir)"

    for bin in "${BINARIES[@]}"; do
        local src="${SRC_DIR}/${bin}"
        case "$(uname -s)" in
            MINGW*|MSYS*|CYGWIN*) src="${SRC_DIR}/${bin}.exe" ;;
        esac
        [ -f "$src" ] || err "Binário não encontrado no pacote: ${bin}"
        # Preserva o binário anterior no lixo antes de sobrescrever.
        trash "${INSTALL_DIR}/${bin}"
        install -m 0755 "$src" "${INSTALL_DIR}/${bin}"
        ok "Instalado: $(tilde "${INSTALL_DIR}/${bin}")"
    done
}

# Confirma que o binário instalado **executa** (pega linker/arquitetura errados antes do PATH).
verify_binaries() {
    local bin="${INSTALL_DIR}/katu" version
    [ -x "$bin" ] || err "Binário não instalado: $(tilde "$bin")"
    version="$("$bin" --version 2>/dev/null | head -n1)" \
        || err "O binário instalado não executa ($(tilde "$bin")); verifique dependências do sistema"
    ok "Binário responde: ${version}"
}

# Cria a pasta de configuração global (o ficheiro nasce ao usar `katu config set --global`).
setup_global_config() {
    local dir="${XDG_CONFIG_HOME:-$HOME/.config}/local/katu"
    mkdir -p "$dir" 2>/dev/null || true
    ok "Config global: $(tilde "$dir/katu.toml")"
}

# ── PATH ─────────────────────────────────────────────────────────────────────

add_path_line() {
    local file="$1"
    local line="export PATH=\"${INSTALL_DIR}:\${PATH}\""
    local marker="# --- katu path ---"

    grep -qxF "$line" "$file" 2>/dev/null && return 0
    grep -qxF "$marker" "$file" 2>/dev/null && return 0

    if [ -s "$file" ] && [ "$(tail -c1 "$file" | wc -l)" -eq 0 ]; then
        echo "" >> "$file"
    fi
    {
        echo "$marker"
        echo "$line"
    } >> "$file"
    ok "PATH adicionado a $(tilde "$file")"
}

setup_path() {
    [ "${KATU_NO_PATH:-0}" = "1" ] && return 0
    info "Verificando PATH..."
    if printf '%s' "$PATH" | tr ':' '\n' | grep -qxF "$INSTALL_DIR"; then
        ok "$(tilde "$INSTALL_DIR") já está no PATH"
        return 0
    fi

    local touched=0 file
    for file in \
        "${HOME}/.profile" \
        "${HOME}/.bashrc" \
        "${HOME}/.bash_profile" \
        "${HOME}/.zshrc" \
        "${ZDOTDIR:-${HOME}}/.zshrc"; do
        [ -f "$file" ] || continue
        touched=1
        add_path_line "$file"
    done

    # Nenhum rc existente: cria ~/.profile (lido por login shells) para garantir o PATH.
    if [ "$touched" -eq 0 ]; then
        : > "${HOME}/.profile"
        add_path_line "${HOME}/.profile"
    fi

    warn "Reinicie o shell ou rode: export PATH=\"${INSTALL_DIR}:\$PATH\""
}

# ── desinstalação ────────────────────────────────────────────────────────────

uninstall() {
    info "Invalidando binários de $(tilde "$INSTALL_DIR") (movidos para o lixo)..."
    for bin in "${BINARIES[@]}"; do
        trash "${INSTALL_DIR}/${bin}"
        trash "${INSTALL_DIR}/${bin}.exe"
        ok "Removido do PATH: $(tilde "${INSTALL_DIR}/${bin}")"
    done
    ok "Config global e projetos preservados."
    ok "Lixo recuperável em $(tilde "${CACHE_ROOT}/trash")"
}

# ── main ─────────────────────────────────────────────────────────────────────

if [ "$DO_UNINSTALL" -eq 1 ]; then
    uninstall
    exit 0
fi

if [ "$FROM_SOURCE" -eq 0 ]; then
    require curl
    TARGET="${KATU_TARGET:-$(detect_target)}"
    info "Detectado: ${TARGET}"
    VERSION="$(resolve_version)"
    info "Instalando katu ${VERSION} (${TARGET}) em $(tilde "$INSTALL_DIR")..."
    download_release "$TARGET"
else
    info "Instalando katu a partir do source em $(tilde "$INSTALL_DIR")..."
    build_from_source
fi

install_binaries
verify_binaries
setup_global_config
setup_path

echo ""
info "Pronto! Binário instalado:"
echo "  katu        → CLI + TUI (loop possuído, política e memória)"
echo ""
info "Próximos passos:"
echo "  cd ~/meu-projeto"
echo "  katu --init                 # funda .katu/ (audit/, knowledge/, log/, trash/, plan/)"
echo "  katu prime                  # contexto de arranque estático"
echo "  katu                        # abre a TUI (multi-turno)"
echo "  katu run \"...\"              # uma rodada: id da sessão + exit code"
echo "  katu memo ask \"...\"         # consulta a memória (só leitura)"
echo "  katu config list            # configuração efetiva (projeto > global)"
echo ""
echo "  # Config global: $(tilde "${XDG_CONFIG_HOME:-$HOME/.config}/local/katu/katu.toml")"
echo "  # Config do projeto: .katu/katu.toml"
echo ""
if [ "$NO_MEMORY" -eq 1 ]; then
    warn "Binário compilado SEM memória in-process (--no-default-features)."
    warn "Para habilitar: inicialize o submódulo (git submodule update --init --recursive) e reinstale."
fi
info "Docs: https://github.com/${REPO}"
