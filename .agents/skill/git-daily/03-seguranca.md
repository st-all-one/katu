# 03 — Segurança (Credenciais, Segredos e Vazamento)

Fonte: `gitcredentials.adoc`, `transfer-data-leaks.adoc`, `git-config.adoc` +
`config/{transfer,fetch,receive,safe,credential}.adoc`, `gitignore.adoc`,
`gitattributes.adoc`, `git-update-index.adoc`, `git-filter-branch.adoc`, `urls.adoc`, `githooks.adoc`.

## 1. Credential helpers (nunca texto puro)

Sem helper, o fluxo é `GIT_ASKPASS` → `core.askPass` → `SSH_ASKPASS` → prompt.

| Helper | Armazenamento | Recomendação |
|---|---|---|
| `cache` | memória, curto período | ok para sessão |
| `store` | **disco, indefinido** (`~/.git-credentials`) | **NÃO em máquina compartilhada** |
| `libsecret` (Linux) / `osxkeychain` (macOS) / `wincred` (Win) / Git Credential Manager | cofre do SO | **preferir** |
| `git-credential-oauth` / GCM | browser + fundo | recomendado para tokens |

```bash
git config --global credential.helper libsecret
git config --global credential.useHttpPath true      # distinguir por path
git config --global transfer.credentialsInUrl die    # proíbe credencial em URL
```

`credential.helper` vazio zera a lista (override de config inferior). `transfer.credentialsInUrl = warn|die`
proíbe/avisa credenciais em `remote.<name>.url`.

## 2. Vazamento de dados em transferência (transfer-data-leaks.adoc)

Os protocolos fetch/push **não** impedem roubo de dados por um peer malicioso:

- **"have" lines**: a vítima anuncia IDs de objetos; o atacante cria uma ref apontando
  para X e recebe seu conteúdo.
- **Delta contra X**: atacante finge ter X; a vítima manda Y como delta contra X,
  revelando regiões de X.

Mitigação oficial: **mantenha dados privados em outro repositório**. `transfer.hideRefs`
esconde refs mas **não** impede roubo de objetos-alvo; namespaces NÃO são controle de
acesso de leitura.

## 3. Objetos maliciosos / fsck

| Config | Efeito | Fonte |
|---|---|---|
| `transfer.fsckObjects` | default no fetch/receive; aborta objeto malformado, `.GIT`, `.gitmodules` malicioso | config/transfer.adoc |
| `fetch.fsckObjects` / `receive.fsckObjects` | checagem no fetch/receive (`receive` usa quarantine) | config/fetch, config/receive |
| `receive.fsck.<msg-id>` / `skipList` | regras específicas | config/receive.adoc |

```bash
git config --global transfer.fsckObjects true
git config --global receive.fsckObjects true
```
Nota: `fetch.fsckObjects` não deixa o object store limpo (objetos maliciosos podem ser
escritos antes de falhar); `receive.fsckObjects` com quarantine sim.

## 4. Segredos no repositório

**`.gitignore`** — ignore por padrão; commitar só templates:
`*.pem`, `*.key`, `*.p12`, `.env`, `secrets.yaml`, `credentials.json`, `id_rsa`, `.aws/`.
Use `*.env.example` versionado.

**`gitattributes` filter (smudge/clean)** — `filter` nomeia driver com `clean` (checkin)
e `smudge` (checkout). Permite cifrar conteúdo no repo e decriptar no worktree. Filtro
ausente = passthru (projeto continua usável).

**Ocultar track local** (não remove do histórico):
```bash
git update-index --assume-unchanged <file>   # promessa de não mudar; para perf
git update-index --skip-worktree    <file>   # igual, p/ arquivos mantidos no index
```

**Remover segredo já commitado** — perigoso:
- `git filter-branch` é **desencorajado** ("plethora of pitfalls", lento); use
  **git-filter-repo** (`https://github.com/newren/git-filter-repo`).
- `git rm --cached <secret>` remove do index, mas **permanece no histórico**.
- **Risco pós-push**: histórico reescrito tem object names diferentes e **não converge**
  com o original — todos os clones devem refazer fetch; force re-clone e comunique.

## 5. Ownership / safe.directory (CVE repositório local)

Git recusa parsear config/hooks de repo de **outro dono** por padrão:
```bash
git config --global --add safe.directory /caminho/repo
git config --global safe.directory '*'        # opt-out total (INSEGURO)
git config --global safe.bareRepository explicit   # protege contra bare repo embutido
```
`~` e `%(prefix)` são interpolados; valor só respeitado em *protected configuration*.

## 6. Hooks de segurança (githooks.adoc)

| Hook | Uso | Fonte |
|---|---|---|
| `pre-commit` | escanear workspace por segredos (bypass com `--no-verify`) | githooks.adoc:97 |
| `pre-receive` | 1x no push; acesso a `GIT_PUSH_CERT*`; exit ≠0 bloqueia tudo | githooks.adoc:261 |
| `update` | 1x por ref; fast-forward only / ACL | githooks.adoc:302 |
| `post-receive` | auditoria pós-update (validar cert push) | githooks.adoc:414 |

Hooks de push rodam **só no servidor**; `pre-commit` é cliente (não confiável server-side).

## 7. Transporte seguro (urls.adoc)

- `git://` **não autentica** ("use with caution on unsecured networks"). Evitar.
- Prefira `ssh://` ou `https://`. Reescrever push para ssh:
  ```bash
  git config url."ssh://example.org/".pushInsteadOf "git://example.org/"
  ```

## Recomendação

1. Credenciais: `libsecret`/`osxkeychain`/`manager`; **nunca** `store` em máquina compartilhada;
   `transfer.credentialsInUrl=die`.
2. `transfer.fsckObjects=true`, `safe.bareRepository=explicit`, `safe.directory` explícito.
3. Segredos: `.gitignore` + filtro smudge/clean; remoção só via `git filter-repo`, com aviso de rewrite.
4. `pre-commit` (scan) + `pre-receive`/`update` (server, c/ `GIT_PUSH_CERT_*`) + `git push --signed`.
5. Transporte `ssh`/`https`; banir `git://`.
