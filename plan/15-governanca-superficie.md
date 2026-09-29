# E14 — Governança, conhecimento e superfície (transversal)

> **Transversal a todas as fases.** Como o conhecimento e a política são artefactos de primeira
> classe, e como a superfície é contida (DF7).
>
> **Decisões:** DF3, DF7. **Depende de:** todas.
> **Gate permanente:** teto de superfície versionado; um facto, um lar.

---

## 1. Regras do projeto (do zed, §57.1)

Uma regra nova só entra se cumprir **os três critérios**:

1. **Não-óbvia** — alguém familiarizado com o código ainda erraria sem ela.
2. **Repetidamente encontrada** — apareceu mais de uma vez.
3. **Específica o suficiente para agir** — instrução concreta, não princípio vago.

E: `AGENTS.md`/`CLAUDE.md`/`GEMINI.md` são **symlinks** para uma fonte (`.rules` ou
`AGENTS.md`) — o SO não deixa divergir (§57.1). O agente **não** edita as regras em linha; propõe
via PR, o revisor decide. **"Trampas a evitar, não mapas a seguir."**

---

## Tarefas

### E14-T01 ☐ ADRs com alternativas obrigatórias
- **Entregáveis:** `docs/adr/`; template exige `## Alternatives considered` (uma decisão sem o que
  venceu convida a re-litigar, §44).
- **Aceite:** `xtask check-docs` falha se uma ADR não tiver a secção; nenhuma ADR é editada para
  outra decisão — substitui-se com uma nova e mantém-se ligada.

### E14-T02 ☐ Postmortems
- **Entregáveis:** template com Executive summary (30 s) → Impact → Timeline → Root cause →
  **Guardrails added** → **Lessons**; escrito quando um bug é sútil, sistémico e caro.
- **Aceite:** cada postmortem tem um guardrail novo com teste; o bug correspondente tem regressão.

### E14-T03 ☐ Um facto, um lar + orçamentos
- **Entregáveis:** `AGENTS.md` (router curto, < 50 linhas) → `docs/agent-rules.md` →
  `docs/topics/*`; verificação de links (link quebrado no router = violação de startup);
  `xtask check-docs` rejeita duplicação e excesso **ou ausência** de conteúdo.
- **Aceite:** nenhum facto em dois sítios; o router tem ≤ 2 saltos até qualquer regra.

### E14-T04 ☐ `policy/` versionado e revisível
- **Entregáveis:** `policy/` (regras, TOML, em PR) · `.katu/` (runtime, gitignored) · `secrets`
  (fora de ambos, só keyring) — a três-way split do §51.6.
- **Aceite:** `policy/` está sob controlo de versão (teste); `.katu/` nunca é commitado; nenhum
  secret em `.katu/` nem em `policy/`.

### E14-T05 ☐ Teto de superfície
- **Entregáveis:** `xtask check-surface` que reporta crates, tools, gates, regras e artefactos com
  um **teto versionado**; aviso quando se aproxima, falha quando excede.
- **Aceite:** um novo crate/tool/regra sem atualizar o teto falha o CI (a lição do arags §20 e do
  maxima §51.13).

### E14-T06 ☐ Catálogos gerados e docs verificadas
- **Entregáveis:** catálogos (tools, regras, verbos) **gerados** do código; blocos `rust` nas docs
  têm de compilar (`doc-typecheck`); `rust_code` documentado.
- **Aceite:** catálogo reescrito à mão é detetado; exemplo de doc que não compila falha.

### E14-T07 ☐ Referências e slices (linhagem)

> **Este** é o documento onde vive a linhagem; o código não a duplica (§44).

- **Regra:** toda decisão `DFxx` cita a referência que a sustenta; toda referência
  (`_REF/*`, `proposal/*`) é **fonte**, nunca dependência de build.
- **Aceite:** `_REF/` e `proposal/` fora do versionamento de build; nenhum crate depende deles.

---

## Definition of Done (permanente)

- [ ] `xtask check-docs`, `check-surface` verdes.
- [ ] ADRs/postmortems com as secções obrigatórias.
- [ ] `AGENTS.md` router ≤ 50 linhas e links válidos.
- [ ] job `msrv` verde em Rust 1.97.0.

## Anti-checklist

- Não escrever regras de arquitetura nos ficheiros de instrução (mapas apodrecem).
- Não manter arquitetura num diagrama; mantê-la num teste (`xtask check-layers`).
- Não confundir histórico com teste (§52.17).
