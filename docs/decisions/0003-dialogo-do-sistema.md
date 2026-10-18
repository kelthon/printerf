# 0003 — Diálogo do sistema como ação principal

- **Status:** Aceita
- **Data:** 2026-10-17

## Contexto
O PrinterF é um app que **envia arquivos** para impressão, não um driver. Quem
conhece impressoras, drivers, bandejas e permissões é o sistema. No COSMIC,
`cosmic-portals.conf` encaminha `org.freedesktop.impl.portal.Print` para o backend
GTK (`xdg-desktop-portal-gtk`).

## Decisão
O botão principal **Imprimir…** usa o portal XDG:
1. `PreparePrint` abre o diálogo do SO pré-preenchido com as opções do app.
2. `portal::apply_choices` aplica **papel e orientação** escolhidos ao layout;
   cópias/duplex/bandeja ficam com o diálogo (não são “queimados” no spool).
3. O spool é composto e entregue com `Print(fd, token)`.

**Enviar direto para <impressora>** permanece como atalho via libcups (útil para
reimprimir rápido e para testes).

## Consequências
- Funciona com qualquer impressora/driver que o sistema suporte, inclusive em Flatpak.
- O diálogo não fica modal à janela (falta `WindowIdentifier` Wayland via xdg-foreign).
- Se o usuário pedir N-up no diálogo, ele se soma ao N-up do app (documentado no guia).
- Se o portal faltar, o erro sugere “Enviar direto”.

## Alternativas descartadas
- **Só envio direto** — duplicaria a seleção de impressora/driver do SO.
- **Compor depois do diálogo sem prepará-lo antes** — o spool não refletiria papel/orientação escolhidos.
