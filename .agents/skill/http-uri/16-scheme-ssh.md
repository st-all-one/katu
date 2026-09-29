# URI Scheme `ssh:`

## 1. Definição

O scheme `ssh` identifica recursos acessíveis via **Secure Shell (SSH)**, um protocolo de rede criptografado para operações remotas seguras.

## 2. Sintaxe

```
ssh://[user@]host[:port]
```

| Componente | Obrigatório | Descrição |
|------------|-------------|-----------|
| `user` | ❌ | Nome de usuário para autenticação |
| `host` | ✅ | Endereço do servidor (domínio ou IP) |
| `port` | ❌ | Porta (default: 22) |

## 3. Exemplos

```
ssh://user@example.com
ssh://user@example.com:2222
ssh://git@github.com
ssh://root@192.168.1.100
```

## 4. Uso na Web

- **Sub-recursos HTML**: **NÃO** usar `ssh:` para carregar recursos em páginas web. Browsers não suportam `ssh:` como scheme para elementos HTML (img, script, link, etc.).
- **Navegação direta**: pode ser usado na barra de endereços ou como link para abrir cliente SSH, mas o comportamento depende do sistema operacional e aplicações registradas.
- **Contexto desenvolvimento**: comum em URIs de repositórios Git, deployment, e acesso remoto.

## 5. Segurança

- O scheme `ssh://` em si não fornece segurança adicional na web (não é protocolo de transferência HTTP)
- SSH fornece autenticação forte (key-based ou password) e criptografia de sessão
- Não confundir com `https://` — são protocolos diferentes para propósitos diferentes

## 6. Tabela de Schemes Completada

| Scheme | Propósito | Uso na Web |
|--------|-----------|------------|
| `http` / `https` | HTTP (padrão web) | ✅ Universal |
| `ssh` | Secure Shell | ⚠️ Links, não sub-recursos |
| `ws` / `wss` | WebSocket | ✅ Sub-recursos JS |
| `ftp` | File Transfer Protocol | ⚠️ Removido para sub-recursos |
| `file` | Arquivos locais | ⚠️ Apenas contexto local |
| `mailto` | Email | ✅ Links |
| `tel` | Telefone | ✅ Links |
