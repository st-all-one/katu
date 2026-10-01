# Falha fechada

Princípio fundacional DF4: perante incerteza, o sistema **nega** — nunca executa um efeito não
autorizado nem deixa um turno num estado inconsistente.

## Onde se aplica

- `katu-policy`: uma regra `Enforced` sem capacidade concedida nega (ver
  [`../../plan/03-contrato-de-politica.md`](../../plan/03-contrato-de-politica.md)).
- Kernel: a ordem §42 (logar → política → efeito) garante que nada corre sem decisão registada.
- Runtime: sem memória saudável, **recusa arrancar**.

## Verificação

`cargo xtask check` corre a auditoria de política (`policy:audit`) e os testes de contenção.
