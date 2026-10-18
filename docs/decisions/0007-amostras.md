# 0007 — Amostras: picsum.photos e página de teste

- **Status:** Parcialmente substituída por [0011](0011-picsum-presets.md) (foto aleatória)
- **Data:** 2026-10-17

## Contexto
Para testar a impressora o usuário não deveria precisar procurar arquivos.

## Decisão
- **Foto aleatória:** `GET https://picsum.photos/{w}/{h}.jpg?random={seed}` na
  proporção da folha (lado maior 1754 px ≈ 150 DPI em A4), via `ureq` (rustls) em
  `spawn_blocking`, limite de 20 MiB, validação de formato antes de gravar em
  `$XDG_CACHE_HOME/printerf/`.
- **Página de teste:** gerada localmente a 300 DPI (offline): moldura e marcas de
  registro, régua em mm, CMYK+RGB, rampa de cinza 0–100 %, gradientes, linhas de
  1/2/3 px e círculos concêntricos. O documento tem o tamanho físico exato da
  folha, então “Tamanho original” imprime a régua em 1:1.

## Consequências
- `ureq` acrescenta rustls/webpki-roots (sem OpenSSL).
- A foto é a única função que usa rede; tem teste `#[ignore]` próprio.
- A página de teste é gerada na orientação atual; mudar a orientação depois não a regenera.

## Alternativas descartadas
- **`reqwest`** — mais pesado (hyper/tokio-io) para um único GET.
- **Página de teste do CUPS (`/usr/share/cups/data/testprint`)** — depende de filtros e não mostra o layout do app.
