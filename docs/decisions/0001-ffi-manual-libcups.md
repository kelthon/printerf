# 0001 — Bindings manuais para a libcups

- **Status:** Aceita
- **Data:** 2026-10-17

## Contexto
Precisamos listar impressoras, ler status/níveis de tinta via IPP, listar/cancelar
jobs e enviar arquivos ao CUPS. As crates existentes (`cups-sys`) estão sem
manutenção e geram bindings com bindgen, exigindo libclang e headers no build.

## Decisão
Declarar à mão (`src/cups/ffi.rs`) as ~20 funções usadas da API estável da libcups
2.x (`cupsGetDests2`, `cupsPrintFile2`, `cupsGetJobs2`, `cupsCancelJob2`,
`ippNewRequest`/`cupsDoRequest`/`ippFind*`…), com uma camada segura em
`src/cups/mod.rs` (RAII para `ipp_t`, conversão de strings, erros tipados).
`build.rs` linka `-lcups` com `libcups2-dev` ou `-l:libcups.so.2` sem ele.

## Consequências
- Build sem libclang; compila até sem `libcups2-dev` (só o runtime).
- Superfície pequena e auditável; ABI da libcups 2.x é estável desde 1.6.
- Somos responsáveis por manter layouts de `cups_dest_t`/`cups_job_t` corretos —
  coberto pelo teste e2e contra um cupsd real.
- libcups 3 (libcups3/PAPPL) muda a API: exigirá uma nova ADR.

## Alternativas descartadas
- **`cups-sys`** — sem manutenção, bindgen.
- **IPP puro em Rust (`ipp` crate)** — reimplementa auth/socket local/`CUPS_SERVER`
  que a libcups já resolve; mais uma pilha HTTP.
- **Chamar `lp`/`lpstat`** — parsing de texto localizado, frágil.
