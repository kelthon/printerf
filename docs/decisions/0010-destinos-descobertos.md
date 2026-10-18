# 0010 — Destinos descobertos por DNS-SD

- **Status:** Aceita
- **Data:** 2026-10-17

## Contexto
Com impressoras de rede *driverless* (IPP Everywhere), o CUPS lista o mesmo
equipamento duas vezes, por exemplo na L3250:

| Destino | `printer-type` | O que é |
|---|---|---|
| `EPSON_L3250_Series` | `0x500100c` (**DISCOVERED**) | Anúncio DNS-SD; sem fila local até alguém imprimir. Era o padrão do sistema. |
| `EPSON_L3250_Series@EPSON859F92.local` | `0x20900c` (TEMPORARY) | Fila local criada pelo cups-browsed (`implicitclass://`). |

O app escolhia o primeiro (padrão + Epson) e consultava `ipp://localhost/printers/…`,
que não existe → status vazio (“Desconhecido”) e erro na fila. O cupsd também apaga
filas temporárias ociosas, então a fila “some” sozinha.

## Decisão
- `Printer::discovered` a partir do bit `CUPS_PRINTER_DISCOVERED` (0x1000000).
- Ordem: Epson → **filas locais antes de destinos descobertos** → padrão → nome.
- `printer_status` verifica `ippGetStatusCode`; `not-found` vira
  `CupsError::NoLocalQueue`, exibido no painel em vez de “Desconhecido”.
- `list_jobs` trata `not-found` como fila vazia.
- Rótulos: “(rede)” para descobertos, “· padrão” para o padrão.
- Exemplo de diagnóstico: `cargo run --example cups_info`.

## Consequências
- Imprimir num destino descoberto continua possível (a libcups cria a fila na hora;
  o caminho principal é o diálogo do sistema, que já lida com isso).
- Status de um destino só descoberto aparece depois da primeira impressão.

## Alternativas descartadas
- **Consultar o equipamento direto** (`cupsConnectDest`/resolver DNS-SD) — mais FFI e
  rede; a fila do cups-browsed já dá o mesmo status.
