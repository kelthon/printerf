# 0006 — Testes e2e com cupsd privado

- **Status:** Aceita
- **Data:** 2026-10-17

## Contexto
O caminho crítico (spool → CUPS → fila → status) envolve FFI e um serviço do
sistema. Testar contra o CUPS da máquina mexeria na configuração do usuário e não
funcionaria no CI de forma reprodutível.

## Decisão
`tests/common/mod.rs` sobe um **cupsd em espaço de usuário** num tempdir (configs,
spool, logs, socket Unix próprios), aponta a libcups para ele via `CUPS_SERVER` e
cria a fila `PrinterF_E2E` (`raw` → `file:///dev/null`, nenhum papel gasto).
O binário `cupsd` é **copiado** para o tempdir porque o perfil AppArmor do Ubuntu
(`usr.sbin.cupsd`) é associado ao caminho e bloquearia escritas fora de `/var`.

O teste cobre: descoberta, status `Stopped`+`Paused`, envio de spool composto e de
texto, posições na fila, PDF que chegou ao CUPS (`PreserveJobFiles`),
cancelamento, conclusão após `cupsenable` e erro para fila inexistente.

## Consequências
- Hermético: roda local e no CI sem root nem mexer no CUPS do sistema.
- Sem `cups` instalado o teste é pulado — exceto com `PRINTERF_E2E_REQUIRED=1` (CI).
- Filas `raw` estão obsoletas no CUPS 2.x e somem no CUPS 3; trocar por um PPD
  genérico ou por uma impressora IPP virtual (ippeveprinter) quando necessário.
- A GUI não é exercitada (não há driver de UI para libcosmic); a lógica fica na lib.

## Alternativas descartadas
- **Mocks da libcups** — não pegariam erros de layout de structs FFI.
- **Container com CUPS** — mais lento e exige Docker no CI/local.
