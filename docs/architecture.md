# Arquitetura

O PrinterF **não é um driver**. Ele abre o documento, mostra um preview fiel,
compõe as folhas e entrega um PDF ao sistema; o **SO** (portal XDG + CUPS) escolhe
a impressora e aplica o driver certo.

```
 ┌──────────── GUI (bin: src/main.rs, src/app/) — libcosmic ──────────────┐
 │  view() ← Model (App) ← update(Message) → Task<Message> (async)          │
 │                ▲                         │                               │
 │  Subscription: time::every(2s) → Poll    ▼                               │
 └──────────────────────────────────┬──────────────────────────────────────┘
                                    │ chama
 ┌──────────── núcleo (lib: printerf) — sem GUI, testável ─────────────────┐
 │ document  ─► layout::compose_sheet ─► spool (PDF) ─┬─► portal  (D-Bus) ──┼─► diálogo do SO ─► CUPS ─► driver
 │ samples  ─┘   (mesmo código p/ preview e spool)    └─► cups    (libcups) ┼─► CUPS (envio direto)
 │ cups::printer_status / list_jobs ◄──────────────── polling ─────────────┤
 └─────────────────────────────────────────────────────────────────────────┘
```

## Elm / iced / libcosmic

| Peça | Onde | Regra |
|---|---|---|
| **Model** | `struct App` + `Tab` por aba | Impressoras, status/fila, diálogos, arrasto; cada aba tem documento, `PrintSettings`, cache e folhas. |
| **Message** | `enum Message` | Eventos de UI e *resultados* de tarefas assíncronas (`DocLoaded`, `Snapshot`, `PreviewReady`, `NativeDone`…). |
| **update** | `App::update` | Único lugar que muda estado; nunca bloqueia — I/O vira `cosmic::task::future`. |
| **view** | `view_controls` / `view_preview` / `view_queue` | Função pura do Model. |
| **Subscription** | `App::subscription` | Polling do cupsd a cada 2 s enquanto há impressora selecionada ([ADR 0005](decisions/0005-polling-da-fila.md)). |

Invariantes:

* **Preview em duas etapas:** a aba rasteriza as páginas **uma vez** (`Tab::cache`, 72 DPI);
  mudanças só recompõem (`spool::compose_cached`), e arrasto/zoom recompõem só a folha
  atual (`compose_cached_sheet`).
* **Gerações:** cada composição incrementa `Tab::generation`; `Composed` de geração antiga
  é descartado, então nunca aparece preview desatualizado.
* **Respostas obsoletas:** `Snapshot(nome, …)` de impressora já desmarcada é ignorado;
  `snapshot_in_flight` impede empilhar consultas se o cupsd estiver lento.
* **libcups é bloqueante:** tudo passa por `spawn_blocking` em `cups::async_api`.

## Módulos (`src/`)

| Módulo | Responsabilidade |
|---|---|
| `cups/ffi.rs` | ~20 funções da libcups declaradas à mão ([ADR 0001](decisions/0001-ffi-manual-libcups.md)). |
| `cups/mod.rs` | API segura: impressoras (Epson primeiro), status IPP (`printer-state-reasons`, `marker-levels`, bandejas, duplex), fila com posição, envio e cancelamento. |
| `portal.rs` | `org.freedesktop.portal.Print`: `prepare` (diálogo) → `apply_choices` → `print` ([ADR 0003](decisions/0003-dialogo-do-sistema.md)). |
| `document.rs` | PDF via poppler-utils ([ADR 0004](decisions/0004-pdf-via-poppler-utils.md)), imagens, texto (cosmic-text). |
| `layout.rs` | `PrintSettings`, intervalo, ordem de impressão (repetir), rotação, fit/fill/original, N-up, `ItemTransform` (corte/zoom/posição), `SheetGeometry`, `compose_sheet`. Puro. |
| `spool.rs` | Spool composto (WYSIWYG) ou pass-through vetorial ([ADR 0002](decisions/0002-spool-composto.md)). |
| `samples.rs` | Foto aleatória (picsum.photos) e página de teste ([ADR 0007](decisions/0007-amostras.md)). |
| `app/mod.rs` (bin) | Estado global, `Message`, `update`, abas, impressão. |
| `app/menu.rs` | Barra de menus e atalhos ([ADR 0009](decisions/0009-interface-menus-abas-ajuste-livre.md)). |
| `app/tab.rs` | Estado por aba: documento, configurações, cache de páginas, folhas, seleção, hit-testing. |
| `app/view.rs` | Tela inicial, painel lateral com rodapé de impressão, abas + preview interativo, status/fila. |
| `app/dialogs.rs` | Foto aleatória, atalhos, sobre. |

## Dois caminhos de impressão

1. **Imprimir…** (principal): `portal::prepare` abre o diálogo do SO → o usuário escolhe
   impressora/cópias/duplex/papel → o spool é composto com papel/orientação escolhidos →
   `portal::print` entrega o fd. Cópias, duplex e bandeja ficam com o diálogo.
2. **Enviar direto** (atalho): `spool::build` → `cups::print_file` na impressora
   selecionada no painel, com as opções do app traduzidas para atributos IPP.
