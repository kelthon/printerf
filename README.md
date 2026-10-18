<p align="center">
  <img src="res/icons/hicolor/scalable/apps/io.github.kelthon.PrinterF.svg" width="128" alt="PrinterF">
</p>

# PrinterF

> Como o `printf` do C — mas para o papel.

App nativo para o **COSMIC Desktop** (Pop!_OS), em Rust + [libcosmic](https://github.com/pop-os/libcosmic),
para abrir, pré-visualizar, organizar e imprimir PDFs, imagens e textos.
**Não é um driver:** ele prepara o arquivo e o sistema operacional escolhe a impressora
e cuida do driver (diálogo de impressão via XDG Desktop Portal + CUPS).

[![CI](https://github.com/kelthon/PrinterF/actions/workflows/ci.yml/badge.svg)](https://github.com/kelthon/PrinterF/actions/workflows/ci.yml)

## Recursos
- Barra de menus (Arquivo · Editar · Impressora · Exibir · Ajuda), **abas** e atalhos de teclado.
- PDF, PNG, JPEG, WebP e `.txt` com **preview fiel** (o mesmo código gera o preview e o spool).
- **Várias imagens por folha** (1, 2, 4, 6, 8, 9, 12, 16), modo “repetir” para folhas de fotos.
- **Ajuste livre** por imagem: clique, arraste, roda do mouse para tamanho, corte por borda.
- Rotação, ajustar / preencher / tamanho original, intervalo `1-3, 5, 8`, A4/Carta, cópias, duplex, bandeja.
- **Imprimir…** pelo diálogo do sistema, ou **Enviar direto** para uma fila CUPS.
- **Foto aleatória** de [picsum.photos](https://picsum.photos) com tamanhos em cm (pequena,
  média, grande, página inteira, personalizada), quantidade e filtros; **página de teste**
  (CMYK, rampa de cinza, linhas finas, régua em mm).
- Status do hardware (pronta, ocupada, sem papel, tinta baixa, offline, pausada),
  níveis de tinta e fila com posição de cada job — atualizados a cada 2 s.

## Início rápido
```bash
sudo apt install build-essential pkg-config just libcups2-dev libxkbcommon-dev \
    libwayland-dev libexpat1-dev libfontconfig-dev libfreetype-dev poppler-utils cups
just run            # ou: cargo run -- documento.pdf
just ci             # fmt + clippy + testes unitários + e2e
just install        # instala em ~/.local (binário, .desktop, ícone)
```

## Documentação
- [Arquitetura](docs/architecture.md) — fluxo Elm/iced, módulos, caminhos de impressão
- [Decisões (ADRs)](docs/decisions/README.md)
- Guias: [uso](docs/guides/using.md) · [desenvolvimento](docs/guides/development.md) ·
  [testes](docs/guides/testing.md) · [CI e releases](docs/guides/release.md)
- [Referências e créditos](docs/references.md)

## Estrutura
```
src/
  lib.rs        núcleo sem GUI (biblioteca `printerf`)
  cups/         FFI libcups + API segura (impressoras, status IPP, fila, envio)
  portal.rs     diálogo de impressão do sistema (org.freedesktop.portal.Print)
  document.rs   PDF (poppler-utils), imagens, texto (cosmic-text)
  layout.rs     configurações, intervalo, rotação, escala, N-up, composição
  spool.rs      PDF de impressão (composto ou vetorial)
  samples.rs    foto aleatória (picsum) e página de teste
  main.rs, app/  GUI libcosmic (menu, abas, painéis, diálogos)
tests/          e2e contra um cupsd privado
examples/       test_page (PNG da página de teste), cups_info (diagnóstico do CUPS)
res/            ícone e .desktop
docs/           arquitetura, ADRs, guias, referências
```

O ícone deriva do Pop icon theme (System76, CC-BY-SA-4.0) — veja [créditos](docs/references.md#serviços-e-créditos).
