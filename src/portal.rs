//! Diálogo de impressão do sistema via `org.freedesktop.portal.Print` (D-Bus).
//!
//! O PrinterF não é driver: ele prepara o arquivo e o **sistema** escolhe a
//! impressora e cuida do driver. No COSMIC o portal encaminha `Print` para o
//! backend GTK (`xdg-desktop-portal-gtk`, ver `cosmic-portals.conf`).
//!
//! Fluxo:
//! 1. `PreparePrint` abre o diálogo do SO, pré-preenchido com nossas opções; o
//!    usuário escolhe impressora, cópias, duplex, papel, orientação…
//! 2. O portal devolve `settings` + `page_setup` + `token`.
//! 3. Recompomos o spool com o papel/orientação escolhidos ([`apply_choices`]),
//!    para que o arquivo corresponda ao que o diálogo vai imprimir.
//! 4. `Print` recebe o fd do spool + token; o backend entrega ao CUPS com as
//!    opções do diálogo (cópias, duplex, bandeja, intervalo de folhas).

use std::fs::File;
use std::path::Path;

use ashpd::desktop::print::{Orientation, PageSetup, PrintProxy, Settings};

use crate::layout::{Orientation as Ori, PaperSize, PrintSettings};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortalOutcome {
    Printed,
    Cancelled,
}

/// Resposta de `PreparePrint`, reduzida ao que afeta a composição.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub token: u32,
    pub orientation: Option<Ori>,
    pub paper: Option<PaperSize>,
}

/// Reconhece nomes de papel PWG (`iso_a4`), PPD (`A4`) e GTK (`na_letter`).
pub fn paper_from_name(name: &str) -> Option<PaperSize> {
    let n = name.to_ascii_lowercase();
    if n == "a4" || n.starts_with("iso_a4") {
        Some(PaperSize::A4)
    } else if n == "letter" || n.starts_with("na_letter") {
        Some(PaperSize::Letter)
    } else {
        None
    }
}

fn to_ori(o: Orientation) -> Ori {
    match o {
        Orientation::Landscape | Orientation::ReverseLandscape => Ori::Landscape,
        Orientation::Portrait | Orientation::ReversePortrait => Ori::Portrait,
    }
}

/// Aplica as escolhas do diálogo sobre nossas configurações. Cópias e duplex
/// **não** entram no spool: o backend do portal os envia como opções do job.
pub fn apply_choices(settings: &PrintSettings, prepared: &Prepared) -> PrintSettings {
    PrintSettings {
        orientation: prepared.orientation.unwrap_or(settings.orientation),
        paper: prepared.paper.unwrap_or(settings.paper),
        copies: 1,
        duplex: false,
        vector_passthrough: false,
        ..settings.clone()
    }
}

/// Passo 1–2: abre o diálogo do sistema. `Ok(None)` = usuário cancelou.
pub async fn prepare(title: &str, settings: &PrintSettings) -> Result<Option<Prepared>, String> {
    let proxy = PrintProxy::new().await.map_err(|e| format!("portal de impressão indisponível: {e}"))?;

    let orientation = match settings.orientation {
        Ori::Portrait => Orientation::Portrait,
        Ori::Landscape => Orientation::Landscape,
    };
    let copies = settings.copies.to_string();
    let (paper, w_mm, h_mm) = match settings.paper {
        PaperSize::A4 => ("iso_a4", 210.0, 297.0),
        PaperSize::Letter => ("na_letter", 215.9, 279.4),
    };
    let duplex = if settings.duplex { "vertical" } else { "simplex" };

    let print_settings =
        Settings::default().orientation(orientation).paper_format(paper).n_copies(copies.as_str()).duplex(duplex);
    let page_setup = PageSetup::default().name(paper).orientation(orientation).width(w_mm).height(h_mm);

    // Sem WindowIdentifier o diálogo não fica preso à janela (exigiria o handle
    // xdg-foreign da surface Wayland).
    let response = proxy.prepare_print(None, title, print_settings, page_setup, "Imprimir", true).await;
    let prepared = match response.and_then(|r| r.response()) {
        Ok(p) => p,
        Err(ashpd::Error::Response(ashpd::desktop::ResponseError::Cancelled)) => return Ok(None),
        Err(e) => return Err(format!("PreparePrint falhou: {e}")),
    };

    let ps = &prepared.page_setup;
    let paper = [ps.name.as_deref(), ps.ppdname.as_deref(), prepared.settings.paper_format.as_deref()]
        .into_iter()
        .flatten()
        .find_map(paper_from_name);
    let orientation = ps.orientation.or(prepared.settings.orientation).map(to_ori);
    Ok(Some(Prepared { token: prepared.token, orientation, paper }))
}

/// Passo 4: entrega o spool ao sistema.
pub async fn print(spool: &Path, title: &str, token: u32) -> Result<(), String> {
    let proxy = PrintProxy::new().await.map_err(|e| format!("portal de impressão indisponível: {e}"))?;
    let file = File::open(spool).map_err(|e| format!("abrindo spool: {e}"))?;
    proxy
        .print(None, title, &file, Some(token), true)
        .await
        .and_then(|r| r.response())
        .map_err(|e| format!("Print falhou: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_paper_names() {
        assert_eq!(paper_from_name("iso_a4"), Some(PaperSize::A4));
        assert_eq!(paper_from_name("iso_a4_210x297mm"), Some(PaperSize::A4));
        assert_eq!(paper_from_name("A4"), Some(PaperSize::A4));
        assert_eq!(paper_from_name("na_letter"), Some(PaperSize::Letter));
        assert_eq!(paper_from_name("Letter"), Some(PaperSize::Letter));
        assert_eq!(paper_from_name("iso_a5"), None);
    }

    #[test]
    fn dialog_choices_override_layout_but_not_job_options() {
        let ours = PrintSettings { copies: 4, duplex: true, nup: 2, vector_passthrough: true, ..Default::default() };
        let prepared = Prepared { token: 7, orientation: Some(Ori::Landscape), paper: Some(PaperSize::Letter) };
        let s = apply_choices(&ours, &prepared);
        assert_eq!(s.orientation, Ori::Landscape);
        assert_eq!(s.paper, PaperSize::Letter);
        assert_eq!(s.nup, 2, "layout do app é preservado");
        assert_eq!((s.copies, s.duplex), (1, false), "cópias/duplex ficam com o diálogo");
        assert!(!s.vector_passthrough);

        let unchanged = apply_choices(&ours, &Prepared { token: 1, orientation: None, paper: None });
        assert_eq!(unchanged.paper, ours.paper);
        assert_eq!(to_ori(Orientation::ReverseLandscape), Ori::Landscape);
    }
}
