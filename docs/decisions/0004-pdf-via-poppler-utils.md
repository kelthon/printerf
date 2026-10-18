# 0004 — PDF via poppler-utils

- **Status:** Aceita
- **Data:** 2026-10-17

## Contexto
Precisamos do tamanho de cada página e de rasterizar páginas de PDF em DPI
arbitrário (preview 72, spool 300).

## Decisão
`PdfBackend` usa `pdfinfo -f 1 -l N` (tamanhos) e `pdftoppm -singlefile -png`
(uma página por chamada), de `poppler-utils`, presente no Pop!_OS.

## Consequências
- Zero dependências nativas de build; renderização de qualidade (poppler).
- Um processo por página: aceitável para documentos comuns; lento em PDFs enormes.
- `PdfBackend` isola a escolha; trocar de backend não afeta o resto.

## Alternativas descartadas
- **`pdfium-render`** — exige distribuir `libpdfium.so` (não empacotada no Ubuntu).
- **`poppler-rs`** — puxa GLib/cairo e `libpoppler-glib-dev` no build.
- **`mupdf`** — build C pesado, licença AGPL.
