# Guia de testes

| Camada | Onde | Comando | Precisa de |
|---|---|---|---|
| Unitários | `#[cfg(test)]` em cada módulo da lib e em `app::{tab, dialogs}` | `just test` | `poppler-utils`, fontes |
| E2E | `tests/e2e_cups.rs` | `just e2e` | pacote `cups` (cupsd, lpadmin) |
| Rede | `samples::tests::picsum_download_works` (`#[ignore]`) | `just test-network` | internet |

## Diagnóstico do CUPS
`just diag` (ou `cargo run --example cups_info`) mostra o que o PrinterF enxerga:
impressoras, se são “descobertas” na rede, status IPP e fila. Útil quando o painel
mostra algo inesperado.

## Unitários
Cobrem as partes puras e de I/O local: parser de intervalo, grade N-up, `compose_sheet`
(fit/fill/original, rotação, folha parcial), mapeamento de opções IPP, condições de
`printer-state-reasons`, parser do `pdfinfo`, paginação de texto, ida e volta do spool
(gera PDF → reabre com poppler → confere cor e tamanho), escolhas do portal, página de teste,
ajuste livre (corte, zoom, deslocamento, limites), N-up 8/12, modo repetir, hit-testing das
células, documentos com várias imagens, presets e URLs do picsum, formulário da foto aleatória.

## E2E com cupsd privado
`tests/common/mod.rs` sobe um cupsd só para o teste ([ADR 0006](../decisions/0006-e2e-cupsd-privado.md)):

1. tempdir em `$XDG_RUNTIME_DIR` (socket Unix tem limite de 108 bytes no caminho);
2. `cupsd.conf`/`cups-files.conf` próprios (sem auth, `PreserveJobFiles Yes`);
3. cópia do binário `cupsd` (escapa do perfil AppArmor associado a `/usr/sbin/cupsd`);
4. `CUPS_SERVER=<socket>` para a libcups; `lpadmin` cria `PrinterF_E2E` → `/dev/null`.

Nada no CUPS do sistema é alterado e nenhuma página é impressa.

Sem `cups` instalado o teste imprime um aviso e passa; `PRINTERF_E2E_REQUIRED=1`
(definido no CI e em `just e2e`) faz ele falhar.

### Problemas comuns
| Sintoma | Causa / solução |
|---|---|
| `cupsd não abriu o socket` | Veja o log impresso; caminho do tempdir longo demais → defina `XDG_RUNTIME_DIR`. |
| `Permission denied` no log do cupsd | AppArmor: confirme que o teste executa a **cópia** do binário. |
| Teste de texto falha no CI | Faltam fontes: `apt install fonts-dejavu-core`. |

## Testar o diálogo do sistema manualmente
O portal não é automatizável sem sessão gráfica. Roteiro manual:
1. `just run`, clique **Página de teste** → **Imprimir…**.
2. No diálogo, escolha **Imprimir para arquivo (PDF)**, paisagem, 2 cópias.
3. Confira o PDF gerado: orientação paisagem e conteúdo igual ao preview.
