# PrinterF — tarefas comuns. Instale `just` (apt install just) e rode `just --list`.

app_id := "io.github.kelthon.PrinterF"
prefix := env_var_or_default("PREFIX", env_var("HOME") / ".local")

# Lista as receitas
default:
    @just --list

# Compila e roda (debug); passe um arquivo opcional: `just run doc.pdf`
run *args:
    cargo run -- {{args}}

# Formata o código
fmt:
    cargo fmt --all

# Mesmo lint do CI
lint:
    cargo fmt --all --check
    cargo clippy --all-targets -- -D warnings

# Testes unitários
test:
    cargo test --lib --bins

# Testes e2e contra um cupsd privado (precisa do pacote `cups`)
e2e:
    PRINTERF_E2E_REQUIRED=1 cargo test --test e2e_cups -- --nocapture

# Teste que baixa uma foto do picsum.photos (rede)
test-network:
    cargo test --lib -- --ignored picsum

# O que o PrinterF enxerga no CUPS (impressoras, status, fila)
diag:
    cargo run --example cups_info

# Tudo que o CI roda
ci: lint test e2e

# Build otimizado
build-release:
    cargo build --release --locked

# Instala binário, .desktop e ícone em $PREFIX (padrão ~/.local)
install: build-release
    install -Dm755 target/release/printerf {{prefix}}/bin/printerf
    install -Dm644 res/{{app_id}}.desktop {{prefix}}/share/applications/{{app_id}}.desktop
    install -Dm644 res/icons/hicolor/scalable/apps/{{app_id}}.svg {{prefix}}/share/icons/hicolor/scalable/apps/{{app_id}}.svg

# Remove o que `install` colocou
uninstall:
    rm -f {{prefix}}/bin/printerf {{prefix}}/share/applications/{{app_id}}.desktop {{prefix}}/share/icons/hicolor/scalable/apps/{{app_id}}.svg
