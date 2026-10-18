# 0002 — Spool composto como padrão

- **Status:** Aceita
- **Data:** 2026-10-17

## Contexto
O preview precisa ser fiel ao papel (rotação, fit/fill/original, N-up, intervalo),
para PDF, imagem e texto. Deixar os filtros do CUPS fazerem a composição torna o
resultado dependente do `pdftopdf`, diferente do que o app mostrou.

## Decisão
Por padrão: intervalo → rasterizar a 300 DPI → rotação → escala → N-up com o
**mesmo** `layout::compose_sheet` do preview → JPEG q92 → PDF de uma imagem por
página (`pdf-writer`) no tamanho exato da mídia → CUPS com `print-scaling=none`.
Processa folha a folha (memória ≈ `nup` páginas de 300 DPI).

Opção “Enviar PDF vetorial” (só PDF, só envio direto): manda o original e traduz
as configurações em IPP (`page-ranges`, `number-up`, `print-scaling`,
`orientation-requested`).

## Consequências
- WYSIWYG garantido e testável (preview e spool usam a mesma função).
- O mesmo arquivo serve ao CUPS e ao portal.
- Texto em PDF vira raster de 300 DPI (bom para jato de tinta, mas maior e não
  selecionável) — por isso existe o pass-through.
- O CUPS ainda faz o último passo (PDF → raster do dispositivo, color management).

## Alternativas descartadas
- **Gerar PWG-Raster/URF no app** — perderíamos dithering/perfis do driver.
- **Compor vetorialmente (reescrever PDFs)** — complexo para mesclar imagens e texto.
