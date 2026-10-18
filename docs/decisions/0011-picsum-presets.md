# 0011 — Foto aleatória: presets, tamanho físico e opções

- **Status:** Aceita (substitui a parte “Foto aleatória” da [0007](0007-amostras.md))
- **Data:** 2026-10-17

## Contexto
A foto aleatória vinha sempre no tamanho da folha, sem opções.

## Decisão
Diálogo “Foto aleatória” (`samples::PicsumOptions`), tudo opcional:

| Opção | Valores | Padrão |
|---|---|---|
| Tamanho | Pequena 480 px (~8×5 cm) · **Média 900 px (~15×10 cm)** · Grande 1500 px (~25×17 cm) · Página inteira · Personalizada (16–5000 px) | Média |
| Formato | Paisagem 3:2 · Retrato 2:3 · Quadrada | Paisagem |
| Quantidade | 1–16 fotos diferentes (baixadas em paralelo) | 1 |
| Preto e branco / Desfoque | `?grayscale`, `?blur=1..10` | não |
| Foto específica | `/id/{id}` | aleatória |

As fotos ganham tamanho físico a **150 DPI**, então os presets correspondem às medidas
indicadas. Ao abrir: 1 foto → “Tamanho original”; página inteira → “Preencher”;
várias → “Ajustar” com N-up que caiba todas. Com uma aba de imagens ativa, as fotos
são **adicionadas** a ela.

## Consequências
- Custom é limitado ao máximo do picsum (5000 px).
- ID específico força quantidade 1 (para repetir, use “Repetir em todas as células”).
