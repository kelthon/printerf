# 0005 — Polling da fila a cada 2 s

- **Status:** Aceita
- **Data:** 2026-10-17

## Contexto
A UI mostra status do hardware e a fila com posição dos jobs, atualizados “ao vivo”.

## Decisão
`Subscription` com `iced::time::every(2s)` dispara `Poll`, que faz uma ida ao cupsd
(`Get-Printer-Attributes` + `cupsGetJobs2`) em `spawn_blocking`. Só há polling com
impressora selecionada e nunca duas consultas simultâneas.

## Consequências
- Simples e robusto; custo desprezível (socket local).
- Latência de até 2 s.
- Impressoras USB que só atualizam `marker-levels` quando consultadas funcionam.

## Alternativas descartadas
- **`Create-Printer-Subscription` + `ippget`** — mais código e estado (renovar lease).
- **Notificador D-Bus do cupsd** — exige configuração no sistema.
