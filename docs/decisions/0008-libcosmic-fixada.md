# 0008 — libcosmic fixada por commit; lib + bin separados

- **Status:** Aceita
- **Data:** 2026-10-17

## Contexto
A libcosmic não é publicada no crates.io e sua API muda entre commits
(ex.: `widget::column()` passou a exigir argumento).

## Decisão
- Dependência git com `rev` fixo no `Cargo.toml`; o Dependabot a ignora e a
  atualização é manual ([guia](../guides/development.md#atualizar-a-libcosmic)).
- O crate tem **lib** (`printerf`: CUPS, portal, documentos, layout, spool,
  amostras — sem GUI) e **bin** (`main.rs` + `app/`: libcosmic).

## Consequências
- Builds reprodutíveis; `Cargo.lock` versionado.
- Testes unitários e e2e importam só a lib — rápidos e sem display.
