mod app;

fn main() -> cosmic::iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "printerf=info".into()),
        )
        .init();

    // `printerf a.pdf b.png …` abre direto (útil para "Abrir com…").
    let files = std::env::args_os().skip(1).map(std::path::PathBuf::from).collect();
    app::run(files)
}
