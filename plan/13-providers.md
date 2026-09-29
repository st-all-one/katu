# E12 — Camada de providers (commodity)

> **Fase 8.** A camada fina sobre o GDK/declarativo — **apenas os providers que importam**. É
> commodity: reaproveitar, não possuir (§2).
>
> **Decisões:** DF6, DF4, DF5. **Depende de:** E05.
> **Gate do épico:** firewall LLM-free intacta; tiers determinístico→caro; nenhum provider no
> grafo de dependências do núcleo.

---

## Princípios

1. **Firewall LLM-free** (§21): `katu-core`, `katu-policy` e `katu-tools` **não podem** depender de
   crates de provider. O modelo é cliente do plano de dados.
2. **Tiers com latência/custo conhecidos:** o determinístico resolve primeiro; o LLM é opt-in, e a
   **política** escolhe o nível (§22).
3. **Não reinventar o commodity:** wire formats, registo de modelos e compaction vêm do GDK ou de
   camada declarativa (§5), isolados atrás de um trait próprio.
4. **Uma capacidade, um provedor:** exatamente um caminho por provider; sem legado em paralelo.

---

## Tarefas

### E12-T01 ☐ Port `Provider` e adaptador mínimo
- **Entregáveis:** trait `Provider` com streaming normalizado, tool calling e contagem de
  custo/tokens; **um** provider no MVP (o mais barato de integrar).
- **Aceite:** o núcleo compila com a feature do provider desligada; `xtask check-layers` falha se
  um crate de provider entrar em `core`/`policy`/`tools`.

### E12-T02 ☐ Integração com GDK/declarativo (commodity)
- **Entregáveis:** uso de `goose-provider-types` / `goose-context-management` (ou equivalente) para
  message/conversation/formatos/compaction, isolados atrás de trait próprio; version pinada.
- **Aceite:** trocar a fonte de commodity muda só o adaptador; nenhum tipo externo na API do katu.

### E12-T03 ☐ Custo/tokens e tiers
- **Entregáveis:** contabilização por chamada; seleção de tier pela política; `Metric` com base de
  evidência (`provider_reported` quando vier do provider, `inferred` quando estimado).
- **Aceite:** custo reportado usa a base correta; `unpriced` para modelo sem preço público; nunca
  inventar preço (DF5).

### E12-T04 ☐ Timeout, retry e cancelamento
- **Entregáveis:** timeout tipado; retry/backoff só em operação idempotente; cancelamento que
  atinge quiescência (§43).
- **Aceite:** provider que trava é cancelado sem vazar tarefa; retry não duplica efeito.

### E12-T05 ☐ Testes com provider fake e snapshot
- **Entregáveis:** provider fake determinístico para o loop (E04/E05); replay de sessão gravada
  sem chave; política explícita "inference is cheap here — não racionar" nos e2e com chave.
- **Aceite:** todo teste de loop corre sem rede; o e2e com chave auto-*skip* sem credencial.

---

## Definition of Done

- [ ] E12-T01…T05 concluídas.
- [ ] Firewall LLM-free verificada pelo `xtask check-layers`.
- [ ] Um provider a funcionar ponta-a-ponta com o loop.
- [ ] `cargo xtask check` e job `msrv` verdes.

## Não-objetivos

- 45 providers, OAuth complexo, inferência local, gateways de plataforma (§0).
- Retomar o `pi-rs` (portar o commodity inteiro): **não** — é o oposto da regra commodity §2.
