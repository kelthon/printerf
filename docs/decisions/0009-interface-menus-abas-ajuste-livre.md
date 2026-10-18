# 0009 — Interface: barra de menus, abas e ajuste livre no preview

- **Status:** Aceita
- **Data:** 2026-10-17

## Contexto
A primeira interface tinha um cabeçalho com muitos botões (Abrir, Foto aleatória,
Página de teste, título, Enviar direto, Imprimir), poluído e pouco intuitivo. Faltavam
abas, várias imagens por folha com controle fino, e um jeito de cortar/redimensionar.

## Decisão
- **Cabeçalho = só a barra de menus** (`menu::bar` da libcosmic): Arquivo · Editar ·
  Impressora · Exibir · Ajuda, com atalhos (`menu::key_binds`) disparados por uma
  subscription de teclado (`event::listen_with`, só eventos não capturados por campos de texto).
- **Abas** (`tab_bar` + `SingleSelectModel`): uma por documento; cada aba guarda seu
  `Tab` (documento, `PrintSettings`, cache de páginas, folhas, seleção).
- **Painel lateral mantido**, com **rodapé fixo** “Imprimir…” (diálogo do SO) e
  “Enviar direto para …”. As ações saíram do cabeçalho.
- **Tela inicial** com 3 cartões (Abrir, Foto aleatória, Página de teste) quando não há abas.
- **Ajuste livre por página** (`layout::ItemTransform`: corte por borda, zoom 10–400 %,
  deslocamento): clicar seleciona a célula, arrastar move, a roda do mouse muda o tamanho,
  e sliders no painel cortam. Persistido em `PrintSettings::transforms` → vale para o
  preview e para o spool.
- **Várias imagens por folha:** N-up 1/2/4/6/8/9/12/16; abrir várias imagens cria uma
  aba com uma página por imagem; “Repetir em todas as células” faz folhas de fotos.
- **Preview rápido:** cada aba rasteriza as páginas **uma vez** (72 DPI, `Tab::cache`);
  mudanças só recompõem (`spool::compose_cached`) e arrastos recompõem só a folha atual
  (`compose_cached_sheet`). Gerações descartam resultados obsoletos.

## Consequências
- Geometria da folha centralizada em `layout::SheetGeometry` (composição, hit-testing,
  conversão de arrasto) — testada na lib e em `app::tab`.
- “Tamanho original” passou a ser **sempre** o tamanho físico, também em N-up
  (antes encolhia por célula), para que fotos em cm saiam no tamanho certo.
- A interação de arrastar/zoom não tem teste automatizado de GUI; a lógica por trás tem.

## Alternativas descartadas
- **Toolbar de ícones** — os menus são mais descobríveis e mostram atalhos.
- **Canvas com alças de redimensionamento** — mais código de widget; arrastar + roda +
  sliders cobre o caso com menos superfície.
