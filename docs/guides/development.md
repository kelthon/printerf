# Guia de desenvolvimento

## Ambiente (Pop!_OS / Ubuntu 24.04)
```bash
sudo apt install build-essential pkg-config just \
    libcups2-dev libxkbcommon-dev libwayland-dev libexpat1-dev \
    libfontconfig-dev libfreetype-dev poppler-utils cups
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # Rust stable ≥ 1.85
```

## Comandos
| Tarefa | Comando |
|---|---|
| Rodar | `just run` ou `cargo run -- arquivo.pdf` |
| Formatar | `just fmt` |
| Lint (igual ao CI) | `just lint` |
| Testes unitários | `just test` |
| Testes e2e | `just e2e` |
| Tudo do CI | `just ci` |
| Instalar em `~/.local` | `just install` |
| Gerar página de teste em PNG | `cargo run --example test_page -- out.png 300` |
| Diagnóstico do CUPS | `just diag` |

Logs: `RUST_LOG=printerf=debug cargo run`.

## Estilo
- `rustfmt.toml`: largura 120, heurísticas `Max`.
- `clippy`: lints em `[lints]` do `Cargo.toml` (`unwrap_used` = warn fora de testes),
  `clippy.toml` com `msrv`. O CI roda `-D warnings`.
- Comentários e mensagens de UI em português; identificadores em inglês.
- Lógica nova vai para a **lib** (`src/*.rs`, fora de `src/app/`) com teste; `src/app/`
  só orquestra Messages e desenha.

## Onde mexer
| Quero… | Arquivo |
|---|---|
| Novo controle na UI | `app/mod.rs`: variante em `Message` → `update`; desenho em `app/view.rs`. Estado de documento vai em `app/tab.rs`. |
| Novo item de menu/atalho | `app/menu.rs`: `Action`, `bar`, `key_binds`, `SHORTCUTS`. |
| Nova opção de impressão | `layout::PrintSettings` + `cups_options_*` + teste. |
| Novo formato de arquivo | `document::{kind_for, open, render_pages}`. |
| Novo atributo IPP de status | `cups::printer_status` (adicione em `requested`). |

## Atualizar a libcosmic
A dependência é fixada por commit ([ADR 0008](../decisions/0008-libcosmic-fixada.md)):
1. Pegue o commit novo em <https://github.com/pop-os/libcosmic/commits/master>.
2. Troque `rev` no `Cargo.toml`, rode `cargo update -p libcosmic` e `just ci`.
3. Quebras de API comuns: construtores de widgets, `Task`/`Action`, `settings::item`.
   Consulte `examples/` do próprio repositório da libcosmic nesse commit.
