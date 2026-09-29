# 02 — Autenticidade (Proveniência e Integridade)

Fonte: `gitformat-signature.adoc`, `git-config.adoc` + `config/{gpg,user,commit,tag}.adoc`,
`git-commit.adoc`, `git-tag.adoc`, `git-push.adoc`, `git-receive-pack.adoc`, `gitfaq.adoc`.

## Por que assinar

Assinar produz **prova criptográfica de autor e integridade** do objeto (commit, tag,
mergetag, push). Limite importante: a assinatura vive *no objeto* — **não há imposição
de servidor por padrão**. O transporte (fetch/push) não valida assinaturas automaticamente;
a verificação é responsabilidade do cliente e de hooks.

> `Signed-off-by` **não é** assinatura. É um trailer DCO (declaração legal de origem/licença),
> não prova criptográfica. Git inclusive recusa `commit.signoff` automático para proteger
> sua credibilidade legal.

## Backends e configuração base

| Config key | Efeito |
|---|---|
| `gpg.format` | `openpgp` (default) \| `x509` \| `ssh` — seleciona backend |
| `user.signingKey` | chave usada por `git commit`/`git tag` (SSH aceita path da pública, `key::...`) |
| `gpg.program` / `gpg.<format>.program` | binário (`gpg`, `gpgsm`, `ssh-keygen`) |
| `gpg.minTrustLevel` | `undefined`/`never`/`marginal`/`fully`/`ultimate` (só verificação local) |
| `gpg.ssh.allowedSignersFile` | arquivo de chaves públicas confiáveis (formato ALLOWED SIGNERS); sem ele, `verify` falha em SSH |
| `gpg.ssh.revocationFile` | lista de chaves revogadas (KRL) |
| `gpg.ssh.defaultKeyCommand` | lookup dinâmico de chave se `user.signingKey` ausente (ex.: `ssh-add -L`) |
| `commit.gpgSign` | assina TODOS os commits (use agent para não digitar passphrase) |
| `tag.gpgSign` / `tag.forceSignAnnotated` | assina TODAS as tags / tags anotadas |

## Exemplo: assinar com SSH (moderno, sem GPG)

```bash
git config --global gpg.format ssh
git config --global user.signingKey "~/.ssh/id_ed25519.pub"
git config --global gpg.ssh.allowedSignersFile "~/.git_allowed_signers"
# ~/.git_allowed_signers:  Nome <email> ssh-ed25519 AAAA...
```

## Comandos de assinatura e verificação

| Comando | Ação | Fonte |
|---|---|---|
| `git commit -S[<keyid>]` / `--gpg-sign` | assina o commit | git-commit.adoc:394 |
| `git tag -s` / `-u <key-id>` | tag assinada | git-tag.adoc:67,80 |
| `git tag -v` / `git verify-tag` | verifica tag | gitformat-signature.adoc:87 |
| `git verify-commit` | verifica commit | gitformat-signature.adoc:135 |
| `git log --show-signature` | exibe/verifica em log | gitformat-signature.adoc:135,200 |

```bash
git commit -S -m "feat: ..."      # assina
git tag -s v1.0 -m "release"      # tag assinada
git log --show-signature          # inspeciona assinaturas
git verify-commit HEAD            # checagem criptográfica
```

## Formatos de assinatura (gitformat-signature.adoc)

O Git monta o payload, chama o programa externo para assinar (destacado) e embute no objeto:

- **PGP**: `-----BEGIN PGP SIGNATURE-----` … `-----END PGP SIGNATURE-----`
- **SSH**: `-----BEGIN SSH SIGNATURE-----` … `-----END SSH SIGNATURE-----`
- **X.509**: `-----BEGIN SIGNED MESSAGE-----` … `-----END SIGNED MESSAGE-----`

- **Tag assinada**: objeto tag anotada + bloco de assinatura *appendado*.
- **Commit assinado**: assinatura no header `gpgsig` (linhas prefixadas por espaço).
- **Mergetag**: ao mesclar uma tag assinada, o objeto tag inteiro é embutido no commit
  de merge como header `mergetag`; aparece em `git show --show-signature`.

## Signed pushes (push certificado)

Prova criptográfica de *quem* fez o push, validável por hooks no servidor:

| Config / Comando | Efeito | Fonte |
|---|---|---|
| `git push --signed[=(true\|false\|if-asked)]` | GPG-assina o pedido de push | git-push.adoc:188 |
| `receive.certNonceSeed` | segredo; `receive-pack` verifica via HMAC nonce | config/receive.adoc |
| `receive.certNonceSlop` | tolerância (s) do nonce | config/receive.adoc |
| `GIT_PUSH_CERT*`, `GIT_PUSH_CERT_STATUS` | expostos a `pre-receive`/`post-receive` | git-receive-pack.adoc:76-119 |

```bash
# servidor
git config receive.certNonceSeed "$(head -c16 /dev/urandom | base64)"
# cliente
git push --signed=if-asked
```
Hooks podem rejeitar se `GIT_PUSH_CERT_STATUS` ≠ `G`.

## Recomendação

1. Habilite `commit.gpgSign=true` + `tag.gpgSign=true` (com agent/keychain).
2. SSH é o caminho de menor atrito: exige `gpg.ssh.allowedSignersFile` para `verify`.
3. Verifique antes de confiar: `git log --show-signature` em CI/review.
4. Para trilha de auditoria de push, combine com `--signed` + hooks server-side.
