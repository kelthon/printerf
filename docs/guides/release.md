# CI, releases e instalação

## Workflow `.github/workflows/ci.yml`
Roda em push para `main`, PRs, tags `v*` e manualmente:

| Job | O quê |
|---|---|
| `fmt` | `cargo fmt --check` |
| `clippy` | `cargo clippy --all-targets -D warnings` |
| `test` | unitários + e2e com cupsd privado (`PRINTERF_E2E_REQUIRED=1`) |
| `build` | `cargo build --release`, empacota `printerf-<ref>-x86_64-linux.tar.gz` (binário + `res/`) como artefato |

Em tags `v*` o `build` também cria um GitHub Release com o tarball.
Dependabot (`.github/dependabot.yml`) atualiza crates e actions semanalmente
(exceto libcosmic).

## Publicar no GitHub
```bash
git init -b main && git add . && git commit -m "PrinterF inicial"
gh repo create PrinterF --public --source . --push
```

## Fazer um release
```bash
# atualize version no Cargo.toml
git tag v0.1.0 && git push origin v0.1.0
```

## Instalar
Do código-fonte: `just install` (em `~/.local`; `PREFIX=/usr/local sudo -E just install` para o sistema).

Do tarball do release:
```bash
tar xzf printerf-v0.1.0-x86_64-linux.tar.gz && cd printerf-v0.1.0-x86_64-linux
install -Dm755 printerf ~/.local/bin/printerf
install -Dm644 res/io.github.kelthon.PrinterF.desktop ~/.local/share/applications/
install -Dm644 res/icons/hicolor/scalable/apps/io.github.kelthon.PrinterF.svg ~/.local/share/icons/hicolor/scalable/apps/
```
Dependências de runtime: `libcups2`, `poppler-utils`, `xdg-desktop-portal-gtk` (diálogo de impressão).
