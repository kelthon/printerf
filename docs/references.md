# Referências

## COSMIC / iced
- libcosmic — <https://github.com/pop-os/libcosmic> (exemplos em `examples/`)
- Livro da libcosmic — <https://pop-os.github.io/libcosmic-book/>
- iced (arquitetura Elm) — <https://book.iced.rs/>
- cosmic-text — <https://github.com/pop-os/cosmic-text>

## Impressão
- CUPS Programming Manual — <https://www.cups.org/doc/cupspm.html>
- CUPS API (libcups 2.x) — <https://www.cups.org/doc/api-cups.html>, IPP API — <https://www.cups.org/doc/api-ipp.html>
- `cupsd.conf` / `cups-files.conf` — <https://www.cups.org/doc/man-cupsd.conf.html>, <https://www.cups.org/doc/man-cups-files.conf.html>
- Opções de job (`lp -o`) — <https://www.cups.org/doc/options.html>
- IPP/1.1 (RFC 8011) — <https://www.rfc-editor.org/rfc/rfc8011> (`printer-state`, `printer-state-reasons`, `job-state`)
- PWG 5100.x (`print-scaling`, `media-source`, `marker-levels`) — <https://www.pwg.org/ipp/everywhere.html>
- XDG Desktop Portal: Print — <https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Print.html>
- ashpd — <https://docs.rs/ashpd>
- Ubuntu AppArmor e CUPS — `/etc/apparmor.d/usr.sbin.cupsd`

## Documentos
- pdf-writer — <https://docs.rs/pdf-writer>
- Poppler utils (`pdftoppm`, `pdfinfo`) — <https://poppler.freedesktop.org/>
- image — <https://docs.rs/image>

## Serviços e créditos
- **Lorem Picsum** — <https://picsum.photos> (fotos de Unsplash; usado pelo botão “Foto aleatória”).
- **Ícone** — derivado de `printer.svg` do [Pop icon theme](https://github.com/pop-os/icon-theme),
  © 2016-2017 System76, **CC-BY-SA-4.0** (ou LGPL-3+). Alterações: paleta (corpo índigo,
  LED âmbar, papel creme) e linhas impressas. O ícone derivado é distribuído sob
  CC-BY-SA-4.0. Comparação:

  ![original vs PrinterF](assets/icon-base-vs-printerf.png)

## Ferramentas
- GitHub Actions: `actions/checkout`, `dtolnay/rust-toolchain`, `Swatinem/rust-cache`,
  `actions/upload-artifact`, `softprops/action-gh-release`.
- just — <https://just.systems>
