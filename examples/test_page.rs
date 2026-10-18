//! Gera a página de teste do PrinterF como PNG, sem abrir o app.
//!
//! ```bash
//! cargo run --example test_page -- pagina.png 300
//! ```

use printerf::layout::PrintSettings;
use printerf::samples::render_test_page;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "pagina-de-teste.png".into());
    let dpi: f32 = args.next().map(|s| s.parse()).transpose()?.unwrap_or(150.0);
    render_test_page(&PrintSettings::default(), None, dpi).save(&out)?;
    println!("página de teste salva em {out} ({dpi} DPI)");
    Ok(())
}
