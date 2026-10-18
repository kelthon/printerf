# Registros de decisão (ADRs)

Formato curto: **Contexto → Decisão → Consequências → Alternativas descartadas**.
Uma decisão nova = um arquivo novo numerado; decisões não são reescritas, são
substituídas (`Status: substituída por 00NN`).

| # | Decisão | Status |
|---|---|---|
| [0001](0001-ffi-manual-libcups.md) | Bindings manuais para a libcups em vez de `cups-sys` | Aceita |
| [0002](0002-spool-composto.md) | Spool composto (raster 300 DPI → PDF) como padrão | Aceita |
| [0003](0003-dialogo-do-sistema.md) | Diálogo do SO (portal XDG) como ação principal | Aceita |
| [0004](0004-pdf-via-poppler-utils.md) | PDF via `pdftoppm`/`pdfinfo` atrás de `PdfBackend` | Aceita |
| [0005](0005-polling-da-fila.md) | Polling de 2 s em vez de assinaturas IPP | Aceita |
| [0006](0006-e2e-cupsd-privado.md) | E2E com cupsd privado em espaço de usuário | Aceita |
| [0007](0007-amostras.md) | Amostras: picsum.photos e página de teste local | Parcialmente substituída por 0011 |
| [0008](0008-libcosmic-fixada.md) | libcosmic fixada por commit; lib + bin separados | Aceita |
| [0009](0009-interface-menus-abas-ajuste-livre.md) | Barra de menus, abas, painel com rodapé, ajuste livre no preview | Aceita |
| [0010](0010-destinos-descobertos.md) | Destinos descobertos por DNS-SD (filas locais primeiro, status honesto) | Aceita |
| [0011](0011-picsum-presets.md) | Foto aleatória: presets em cm, personalizado, quantidade, opções | Aceita |

Modelo: copie [template.md](template.md).
