//! Estratégia de spool.
//!
//! Dois caminhos, escolhidos por documento:
//!
//! * **Composto (padrão, WYSIWYG)** — rasteriza cada página-fonte a 300 DPI, aplica
//!   intervalo → rotação → escala → N-up com o *mesmo* [`compose_sheet`] do preview,
//!   codifica cada folha como JPEG e embute num PDF de uma imagem por página
//!   (`pdf-writer`). O CUPS recebe um PDF “burro”, já no tamanho de mídia, com
//!   `print-scaling=none`: nada de surpresas entre preview e papel. Funciona igual
//!   para PDF, imagem e texto, e é o mesmo arquivo entregue ao portal XDG.
//! * **Pass-through vetorial (só PDF)** — envia o PDF original e traduz as
//!   configurações em atributos IPP (`page-ranges`, `number-up`, `print-scaling`,
//!   `orientation-requested`); o `pdftopdf` do cups-filters faz a composição.
//!   Texto/vetores saem nítidos e o spool é minúsculo, mas o resultado depende
//!   do filtro, não do nosso preview.
//!
//! Por que PDF e não PWG-Raster/URF direto? A Epson L-series via driver/IPP
//! Everywhere aceita PDF, e deixar o CUPS fazer o *último* passo (PDF → raster do
//! dispositivo, com color management e dithering do driver) dá melhor qualidade
//! que gerar o raster do dispositivo no app.

use std::io::Cursor;
use std::path::PathBuf;

use anyhow::{Context, Result};
use image::{RgbaImage, codecs::jpeg::JpegEncoder};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref};

use crate::cups::JobOptions;
use crate::document::{self, Document, Kind};
use crate::layout::{PrintSettings, SourcePage, compose_sheet, sheet_count};

pub const SPOOL_DPI: f32 = 300.0;
const JPEG_QUALITY: u8 = 92;

/// Compõe todas as folhas a `dpi` (usado pelo preview).
pub async fn compose_all(doc: Document, settings: PrintSettings, dpi: f32) -> Result<Vec<RgbaImage>> {
    let indices = settings.print_order(doc.page_count()).map_err(anyhow::Error::msg)?;
    let pages = document::render_pages(doc, indices, dpi).await?;
    let n = sheet_count(pages.len(), settings.nup);
    tokio::task::spawn_blocking(move || (0..n).map(|i| compose_sheet(&pages, &settings, i, dpi)).collect())
        .await
        .context("compondo folhas")
}

/// Compõe folhas a partir de páginas já rasterizadas (`cache[i].index == i`).
/// É o caminho rápido do preview: sem I/O, só composição — usado a cada arrasto.
pub fn compose_cached(cache: &[SourcePage], settings: &PrintSettings, dpi: f32) -> Result<Vec<RgbaImage>> {
    let order = settings.print_order(cache.len()).map_err(anyhow::Error::msg)?;
    let pages: Vec<SourcePage> = order.iter().filter_map(|&i| cache.get(i).cloned()).collect();
    Ok((0..sheet_count(pages.len(), settings.nup)).map(|i| compose_sheet(&pages, settings, i, dpi)).collect())
}

/// Recompõe só a folha `sheet` (caminho do arrasto/zoom: o ajuste de uma página
/// afeta apenas a folha em que ela aparece).
pub fn compose_cached_sheet(
    cache: &[SourcePage],
    settings: &PrintSettings,
    sheet: usize,
    dpi: f32,
) -> Result<Option<RgbaImage>> {
    let order = settings.print_order(cache.len()).map_err(anyhow::Error::msg)?;
    let per = settings.nup.max(1) as usize;
    let pages: Vec<SourcePage> =
        order.iter().skip(sheet * per).take(per).filter_map(|&i| cache.get(i).cloned()).collect();
    Ok((!pages.is_empty()).then(|| compose_sheet(&pages, settings, 0, dpi)))
}

pub struct Spool {
    /// Mantém o arquivo vivo até o job ser aceito pelo CUPS/portal.
    pub file: tempfile::TempPath,
    pub options: JobOptions,
    pub sheets: usize,
}

impl Spool {
    pub fn path(&self) -> PathBuf {
        self.file.to_path_buf()
    }
}

pub async fn build(doc: Document, settings: PrintSettings) -> Result<Spool> {
    if settings.vector_passthrough && doc.kind == Kind::Pdf {
        // Cópia (não symlink): o CUPS lê o arquivo depois que retornamos.
        let tmp = tempfile::Builder::new().suffix(".pdf").tempfile()?.into_temp_path();
        tokio::fs::copy(&doc.path, &tmp).await?;
        return Ok(Spool { file: tmp, options: settings.cups_options_passthrough(), sheets: doc.page_count() });
    }

    let indices = settings.print_order(doc.page_count()).map_err(anyhow::Error::msg)?;
    let per = settings.nup.max(1) as usize;
    let (w_pt, h_pt) = settings.sheet_pt();

    // Folha a folha: no máximo `nup` páginas de 300 DPI (~35 MB cada) em memória.
    let mut jpegs = Vec::new();
    for chunk in indices.chunks(per) {
        let pages = document::render_pages(doc.clone(), chunk.to_vec(), SPOOL_DPI).await?;
        let s = settings.clone();
        let jpeg = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, u32, u32)> {
            let sheet = compose_sheet(&pages, &s, 0, SPOOL_DPI);
            let rgb = image::DynamicImage::ImageRgba8(sheet).to_rgb8();
            let mut out = Cursor::new(Vec::new());
            JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY).encode_image(&rgb)?;
            Ok((out.into_inner(), rgb.width(), rgb.height()))
        })
        .await??;
        jpegs.push(jpeg);
    }

    let bytes = tokio::task::spawn_blocking(move || write_pdf(&jpegs, w_pt, h_pt)).await?;
    let tmp = tempfile::Builder::new().prefix("printerf-").suffix(".pdf").tempfile()?;
    tokio::fs::write(tmp.path(), bytes).await?;
    let sheets = sheet_count(indices.len(), settings.nup);
    Ok(Spool { file: tmp.into_temp_path(), options: settings.cups_options_composed(), sheets })
}

/// PDF com uma imagem JPEG (DCTDecode) ocupando cada página inteira.
fn write_pdf(sheets: &[(Vec<u8>, u32, u32)], w_pt: f32, h_pt: f32) -> Vec<u8> {
    let mut pdf = Pdf::new();
    let mut alloc = Ref::new(1);
    let catalog = alloc.bump();
    let tree = alloc.bump();
    let mut page_ids = Vec::with_capacity(sheets.len());

    for (jpeg, w, h) in sheets {
        let (page_id, img_id, content_id) = (alloc.bump(), alloc.bump(), alloc.bump());
        page_ids.push(page_id);

        let mut page = pdf.page(page_id);
        page.media_box(Rect::new(0.0, 0.0, w_pt, h_pt));
        page.parent(tree);
        page.contents(content_id);
        page.resources().x_objects().pair(Name(b"Im0"), img_id);
        page.finish();

        let mut img = pdf.image_xobject(img_id, jpeg);
        img.filter(Filter::DctDecode);
        img.width(*w as i32);
        img.height(*h as i32);
        img.color_space().device_rgb();
        img.bits_per_component(8);
        img.finish();

        let mut c = Content::new();
        c.save_state();
        c.transform([w_pt, 0.0, 0.0, h_pt, 0.0, 0.0]);
        c.x_object(Name(b"Im0"));
        c.restore_state();
        pdf.stream(content_id, &c.finish());
    }

    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree).kids(page_ids.iter().copied()).count(page_ids.len() as i32);
    pdf.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    #[tokio::test]
    async fn text_spool_is_valid_pdf_with_nup() {
        let dir = tempfile::tempdir().unwrap();
        let txt = dir.path().join("t.txt");
        std::fs::write(&txt, (1..=200).map(|i| format!("linha {i} — ação\n")).collect::<String>()).unwrap();
        let doc = document::open(txt).await.unwrap();
        assert!(doc.page_count() >= 3, "200 linhas devem ocupar várias páginas A4");

        let settings = PrintSettings { nup: 2, page_range: "1-3".into(), ..Default::default() };
        let spool = build(doc, settings).await.unwrap();
        assert_eq!(spool.sheets, 2);

        let out = std::process::Command::new("pdfinfo").arg(spool.path()).output().unwrap();
        let info = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        assert!(info.contains("Pages:           2"), "{info}");
        assert!(info.contains("595.28 x 841.89"), "{info}");
    }

    #[tokio::test]
    async fn spool_roundtrips_as_pdf_document() {
        // Gera um spool de imagem e reabre como PDF: valida pdf-writer + backend poppler.
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("a.png");
        RgbaImage::from_pixel(50, 50, image::Rgba([255, 0, 0, 255])).save(&png).unwrap();
        let settings = PrintSettings { orientation: crate::layout::Orientation::Landscape, ..Default::default() };
        let spool = build(document::open(png).await.unwrap(), settings).await.unwrap();

        let pdf = dir.path().join("spool.pdf");
        std::fs::copy(spool.path(), &pdf).unwrap();
        let doc = document::open(pdf).await.unwrap();
        assert_eq!(doc.kind, Kind::Pdf);
        assert_eq!(doc.page_count(), 1);
        let (w, h) = doc.page_sizes[0];
        assert!((w - 841.89).abs() < 0.1 && (h - 595.28).abs() < 0.1, "{w}x{h}");

        let page = &document::render_pages(doc, vec![0], 36.0).await.unwrap()[0].image;
        let c = page.get_pixel(page.width() / 2, page.height() / 2).0;
        assert!(c[0] > 200 && c[1] < 60 && c[2] < 60, "centro deveria ser vermelho: {c:?}");
    }

    #[tokio::test]
    async fn passthrough_keeps_original_pdf() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.pdf");
        std::fs::write(&src, write_pdf(&[], 595.28, 841.89)).unwrap();
        let doc = Document { path: src.clone(), kind: Kind::Pdf, page_sizes: vec![(595.28, 841.89)], images: vec![] };
        let spool = build(doc, PrintSettings { vector_passthrough: true, nup: 2, ..Default::default() }).await.unwrap();
        assert_eq!(std::fs::read(spool.path()).unwrap(), std::fs::read(&src).unwrap());
        assert!(spool.options.0.iter().any(|(k, v)| k == "number-up" && v == "2"));
    }

    #[tokio::test]
    async fn invalid_range_is_an_error() {
        let doc = Document::image("/x.png".into(), (1.0, 1.0));
        let err = build(doc, PrintSettings { page_range: "2".into(), ..Default::default() }).await.err().unwrap();
        assert!(err.to_string().contains("fora do intervalo"), "{err}");
    }

    #[test]
    fn compose_cached_follows_print_order() {
        let page = |index, c| SourcePage {
            index,
            image: RgbaImage::from_pixel(10, 10, image::Rgba(c)),
            width_pt: 10.0,
            height_pt: 10.0,
        };
        let cache = vec![page(0, [255, 0, 0, 255]), page(1, [0, 0, 255, 255])];
        let s = PrintSettings { nup: 2, repeat: true, page_range: "2".into(), ..Default::default() };
        let sheets = compose_cached(&cache, &s, 36.0).unwrap();
        assert_eq!(sheets.len(), 1);
        let (w, h) = sheets[0].dimensions();
        assert_eq!(sheets[0].get_pixel(w / 2, h / 4).0, [0, 0, 255, 255], "repetida: página 2 nas duas células");
        assert_eq!(sheets[0].get_pixel(w / 2, 3 * h / 4).0, [0, 0, 255, 255]);

        let one = compose_cached_sheet(&cache, &s, 0, 36.0).unwrap().unwrap();
        assert_eq!(one.as_raw(), sheets[0].as_raw(), "folha isolada = folha do conjunto");
        assert!(compose_cached_sheet(&cache, &s, 5, 36.0).unwrap().is_none());
    }

    #[tokio::test]
    async fn spool_repeat_makes_one_sheet_per_image() {
        let dir = tempfile::tempdir().unwrap();
        let mut paths = vec![];
        for i in 0..2 {
            let p = dir.path().join(format!("{i}.png"));
            RgbaImage::from_pixel(20, 20, image::Rgba([0, 0, 0, 255])).save(&p).unwrap();
            paths.push(p);
        }
        let doc = document::open_many(paths).await.unwrap().remove(0);
        let s = PrintSettings { nup: 4, repeat: true, ..Default::default() };
        let spool = build(doc, s).await.unwrap();
        assert_eq!(spool.sheets, 2);
    }
}
