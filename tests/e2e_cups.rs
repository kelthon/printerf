//! End-to-end: documento → composição → spool → CUPS (cupsd privado) → fila → status.
//!
//! Exercita exatamente o caminho do botão “Enviar direto” do app, sem GUI e sem
//! gastar papel. Um único `#[test]` porque libcups guarda `CUPS_SERVER` por thread
//! e o cupsd privado é caro de subir.

mod common;

use common::{PrivateCups, QUEUE, wait_until};
use printerf::cups::{self, Condition, JobState, PrinterState};
use printerf::document;
use printerf::layout::PrintSettings;
use printerf::{samples, spool};

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("runtime tokio")
}

#[test]
fn print_lifecycle_against_private_cupsd() {
    let Some(server) = PrivateCups::start() else { return };
    let rt = runtime();

    // 1. Descoberta de impressoras
    let printers = cups::list_printers().unwrap();
    assert!(printers.iter().any(|p| p.name == QUEUE), "{printers:?}");

    // 2. Status: fila pausada → Stopped + condição Paused
    server.admin("cupsdisable", &[QUEUE]);
    let status = cups::printer_status(QUEUE).unwrap();
    assert_eq!(status.state, PrinterState::Stopped);
    assert!(status.conditions.contains(&Condition::Paused), "{:?}", status.conditions);
    assert_eq!(status.summary(), "Pausada");

    // 3. Spool composto da página de teste (2 por folha, paisagem) → job 1
    let settings = PrintSettings {
        nup: 2,
        orientation: printerf::layout::Orientation::Landscape,
        copies: 2,
        ..Default::default()
    };
    let (job1, sheets) = rt.block_on(async {
        let doc = samples::test_page(&PrintSettings::default(), Some(QUEUE.into())).await.unwrap();
        let spool = spool::build(doc, settings.clone()).await.unwrap();
        let id = cups::async_api::print_file(
            QUEUE.into(),
            spool.path(),
            "e2e página de teste".into(),
            spool.options.clone(),
        )
        .await
        .unwrap();
        (id, spool.sheets)
    });
    assert!(job1 > 0);
    assert_eq!(sheets, 1);

    // 4. Texto com intervalo de páginas → job 2
    let job2 = rt.block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let txt = dir.path().join("relatorio.txt");
        std::fs::write(&txt, "linha de teste\n".repeat(200)).unwrap();
        let doc = document::open(txt).await.unwrap();
        let s = PrintSettings { page_range: "1-2".into(), ..Default::default() };
        let spool = spool::build(doc, s).await.unwrap();
        cups::print_file(QUEUE, &spool.path(), "e2e texto", &spool.options).unwrap()
    });

    // 5. Fila: ambos pendentes, na ordem de envio
    let jobs = cups::list_jobs(QUEUE, false).unwrap();
    let pos = |id| jobs.iter().find(|j| j.id == id).and_then(|j| j.queue_position);
    assert_eq!((pos(job1), pos(job2)), (Some(1), Some(2)), "{jobs:?}");
    assert!(jobs.iter().all(|j| j.state == JobState::Pending));

    // 6. O que chegou ao CUPS é o PDF composto: 1 folha paisagem A4
    let spooled = std::fs::read(server.spooled_document(job1)).unwrap();
    assert!(spooled.starts_with(b"%PDF-"), "spool não é PDF");
    let info = std::process::Command::new("pdfinfo").arg(server.spooled_document(job1)).output().unwrap();
    let info = String::from_utf8_lossy(&info.stdout);
    assert!(info.contains("Pages:           1"), "{info}");
    assert!(info.contains("841.89 x 595.28"), "{info}");

    // 7. Cancelar job 2 → job 1 continua em 1º
    rt.block_on(cups::async_api::cancel_job(QUEUE.into(), job2)).unwrap();
    let jobs = cups::list_jobs(QUEUE, true).unwrap();
    let j2 = jobs.iter().find(|j| j.id == job2).unwrap();
    assert_eq!(j2.state, JobState::Canceled);
    assert_eq!(jobs.iter().find(|j| j.id == job1).unwrap().queue_position, Some(1));

    // 8. Reativar → job 1 conclui; snapshot assíncrono (o que a UI consome) reflete isso
    server.admin("cupsenable", &[QUEUE]);
    wait_until(20, "job 1 concluir", || {
        cups::list_jobs(QUEUE, true).unwrap().iter().any(|j| j.id == job1 && j.state == JobState::Completed)
    });
    let (status, jobs) = rt.block_on(cups::async_api::snapshot(QUEUE.into())).unwrap();
    assert_eq!(status.state, PrinterState::Idle, "log:\n{}", server.error_log());
    assert!(jobs.iter().all(|j| !j.state.is_active()));

    // 9. Erros do CUPS chegam como CupsError legível
    let err = cups::print_file("FilaQueNaoExiste", &server.spooled_document(job1), "x", &Default::default());
    assert!(matches!(err, Err(cups::CupsError::Cups(ref m)) if !m.is_empty()), "{err:?}");
}
