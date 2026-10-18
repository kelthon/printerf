//! Diagnóstico: o que o PrinterF enxerga no CUPS (impressoras, status, fila).
//!
//! ```bash
//! cargo run --example cups_info
//! ```

use printerf::cups;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let printers = cups::list_printers()?;
    if printers.is_empty() {
        println!("Nenhuma impressora no CUPS.");
    }
    for p in printers {
        println!("■ {}  (info: {:?}, modelo: {:?}, padrão: {})", p.name, p.info, p.make_and_model, p.is_default);
        match cups::printer_status(&p.name) {
            Ok(s) => println!(
                "  status: {} | estado {:?} | condições {:?} | tinta {:?} | bandejas {:?} | lados {:?}",
                s.summary(),
                s.state,
                s.conditions,
                s.markers.iter().map(|m| (&m.name, m.level)).collect::<Vec<_>>(),
                s.media_sources,
                s.sides
            ),
            Err(e) => println!("  status: ERRO {e}"),
        }
        match cups::list_jobs(&p.name, true) {
            Ok(jobs) => println!(
                "  fila: {} job(s){}",
                jobs.len(),
                jobs.first().map(|j| format!(", último #{} {:?}", j.id, j.state)).unwrap_or_default()
            ),
            Err(e) => println!("  fila: ERRO {e}"),
        }
    }
    Ok(())
}
