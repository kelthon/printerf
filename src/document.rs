//! Abertura e rasterização de documentos-fonte (PDF, imagens, texto puro).
//!
//! PDF: backend `pdftoppm`/`pdfinfo` (poppler-utils, presente no Pop!_OS), isolado
//! atrás de [`PdfBackend`] — dá para trocar por `pdfium-render` (lib dinâmica) ou
//! `poppler-rs` (GLib) sem tocar no resto do app.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::{Context, Result, bail};
use image::RgbaImage;
use tokio::process::Command;

use crate::layout::{PT_PER_MM, PaperSize, SourcePage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Pdf,
    Image,
    Text,
}

#[derive(Debug, Clone)]
pub struct Document {
    /// Arquivo principal (a primeira imagem, em conjuntos de imagens).
    pub path: PathBuf,
    pub kind: Kind,
    /// Tamanho de cada página em pontos.
    pub page_sizes: Vec<(f32, f32)>,
    /// `Kind::Image`: um arquivo por página (várias fotos num mesmo documento).
    pub images: Vec<PathBuf>,
}

impl Document {
    /// Uma imagem com tamanho físico conhecido (em pontos).
    pub fn image(path: PathBuf, size_pt: (f32, f32)) -> Self {
        Self { images: vec![path.clone()], path, kind: Kind::Image, page_sizes: vec![size_pt] }
    }

    pub fn page_count(&self) -> usize {
        self.page_sizes.len()
    }

    pub fn title(&self) -> String {
        let name = self.path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        match self.images.len() {
            n if n > 1 => format!("{name} + {} imagens", n - 1),
            _ => name,
        }
    }

    /// Junta as imagens de `other` a este documento (ambos `Kind::Image`).
    pub fn append_images(&mut self, other: Document) -> Result<()> {
        if self.kind != Kind::Image || other.kind != Kind::Image {
            bail!("só é possível juntar imagens a um documento de imagens");
        }
        self.images.extend(other.images);
        self.page_sizes.extend(other.page_sizes);
        Ok(())
    }

    /// Nome curto da página `i` (arquivo da imagem ou “Página N”).
    pub fn page_label(&self, i: usize) -> String {
        match self.kind {
            Kind::Image => self
                .images
                .get(i)
                .and_then(|p| p.file_name())
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| format!("Imagem {}", i + 1)),
            _ => format!("Página {}", i + 1),
        }
    }
}

pub fn kind_for(path: &Path) -> Option<Kind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "pdf" => Kind::Pdf,
        "png" | "jpg" | "jpeg" | "webp" => Kind::Image,
        "txt" | "text" | "log" | "md" => Kind::Text,
        _ => return None,
    })
}

/// Densidade assumida para imagens sem metadado de DPI (“tamanho original”).
pub const IMAGE_ASSUMED_DPI: f32 = 96.0;

/// Tamanho físico (pt) de uma imagem de `w × h` px a `dpi`.
pub fn image_size_pt(w: u32, h: u32, dpi: f32) -> (f32, f32) {
    (w as f32 * 72.0 / dpi, h as f32 * 72.0 / dpi)
}

/// Abre uma imagem assumindo `dpi` para o tamanho físico.
pub fn open_image_at(path: PathBuf, dpi: f32) -> Result<Document> {
    let (w, h) = image::image_dimensions(&path).with_context(|| format!("lendo {}", path.display()))?;
    Ok(Document::image(path, image_size_pt(w, h, dpi)))
}

pub async fn open(path: PathBuf) -> Result<Document> {
    let kind = kind_for(&path).with_context(|| format!("formato não suportado: {}", path.display()))?;
    let page_sizes = match kind {
        Kind::Pdf => PdfBackend::page_sizes(&path).await?,
        Kind::Image => return tokio::task::spawn_blocking(move || open_image_at(path, IMAGE_ASSUMED_DPI)).await?,
        Kind::Text => {
            let p = path.clone();
            let pages = tokio::task::spawn_blocking(move || text::render(&p, 18.0).map(|v| v.len())).await??;
            vec![PaperSize::A4.size_pt(); pages]
        }
    };
    if page_sizes.is_empty() {
        bail!("documento sem páginas");
    }
    Ok(Document { path, kind, page_sizes, images: vec![] })
}

/// Abre vários arquivos. Só imagens → **um** documento com uma página por
/// imagem (para N-up de fotos). Caso contrário, um documento por arquivo.
pub async fn open_many(paths: Vec<PathBuf>) -> Result<Vec<Document>> {
    let all_images = !paths.is_empty() && paths.iter().all(|p| kind_for(p) == Some(Kind::Image));
    let mut docs = Vec::with_capacity(paths.len());
    for p in paths {
        docs.push(open(p).await?);
    }
    if all_images && docs.len() > 1 {
        let mut it = docs.into_iter();
        let mut first = it.next().context("lista vazia")?;
        for d in it {
            first.append_images(d)?;
        }
        return Ok(vec![first]);
    }
    Ok(docs)
}

/// Lado maior do raster de imagem: 1,5× o lado maior de uma A4 ao DPI pedido.
fn image_raster_limit(dpi: f32) -> u32 {
    (PaperSize::A4.size_pt().1 * 1.5 * dpi / 72.0) as u32
}

/// Rasteriza as páginas `indices` (0-based) a `dpi`.
pub async fn render_pages(doc: Document, indices: Vec<usize>, dpi: f32) -> Result<Vec<SourcePage>> {
    let size = |i: usize| doc.page_sizes.get(i).copied().with_context(|| format!("página {} não existe", i + 1));
    match doc.kind {
        Kind::Pdf => {
            let mut out = Vec::with_capacity(indices.len());
            for index in indices {
                let image = PdfBackend::render_page(&doc.path, index, dpi).await?;
                let (w, h) = size(index)?;
                out.push(SourcePage { index, image, width_pt: w, height_pt: h });
            }
            Ok(out)
        }
        Kind::Image => {
            // Cada imagem é decodificada uma vez, mesmo que apareça repetida.
            let mut cache: std::collections::HashMap<usize, RgbaImage> = Default::default();
            let mut out = Vec::with_capacity(indices.len());
            for index in indices {
                let (w, h) = size(index)?;
                if let std::collections::hash_map::Entry::Vacant(slot) = cache.entry(index) {
                    let path = doc.images.get(index).cloned().unwrap_or_else(|| doc.path.clone());
                    let limit = image_raster_limit(dpi);
                    let img = tokio::task::spawn_blocking(move || -> Result<RgbaImage> {
                        let img = image::open(&path).with_context(|| format!("abrindo {}", path.display()))?;
                        Ok(if img.width().max(img.height()) > limit {
                            img.resize(limit, limit, image::imageops::FilterType::Triangle).to_rgba8()
                        } else {
                            img.to_rgba8()
                        })
                    })
                    .await??;
                    slot.insert(img);
                }
                let image = cache[&index].clone();
                out.push(SourcePage { index, image, width_pt: w, height_pt: h });
            }
            Ok(out)
        }
        Kind::Text => {
            let path = doc.path.clone();
            let all = tokio::task::spawn_blocking(move || text::render(&path, dpi)).await??;
            let (w, h) = PaperSize::A4.size_pt();
            Ok(indices
                .into_iter()
                .filter_map(|index| {
                    all.get(index).cloned().map(|image| SourcePage { index, image, width_pt: w, height_pt: h })
                })
                .collect())
        }
    }
}

// ---------------------------------------------------------------------------

/// Extrai tamanhos de `pdfinfo -f 1 -l N`: `"Page    1 size: 595.276 x 841.89 pts (A4)"`.
fn parse_pdfinfo_sizes(text: &str) -> Vec<(f32, f32)> {
    text.lines()
        .filter(|l| l.starts_with("Page") && l.contains(" size: "))
        .filter_map(|l| {
            let mut it = l.split(" size: ").nth(1)?.split_whitespace();
            let w: f32 = it.next()?.parse().ok()?;
            it.next(); // "x"
            let h: f32 = it.next()?.parse().ok()?;
            Some((w, h))
        })
        .collect()
}

pub struct PdfBackend;

impl PdfBackend {
    async fn page_sizes(path: &Path) -> Result<Vec<(f32, f32)>> {
        // `-l 100000` faz o pdfinfo listar o tamanho de cada página.
        let out = Command::new("pdfinfo")
            .args(["-f", "1", "-l", "100000"])
            .arg(path)
            .output()
            .await
            .context("executando pdfinfo (instale poppler-utils)")?;
        if !out.status.success() {
            bail!("pdfinfo: {}", String::from_utf8_lossy(&out.stderr).trim());
        }
        Ok(parse_pdfinfo_sizes(&String::from_utf8_lossy(&out.stdout)))
    }

    async fn render_page(path: &Path, index: usize, dpi: f32) -> Result<RgbaImage> {
        let dir = tempfile::tempdir()?;
        let prefix = dir.path().join("p");
        let n = (index + 1).to_string();
        let status = Command::new("pdftoppm")
            .args(["-r", &format!("{dpi:.0}"), "-f", &n, "-l", &n, "-singlefile", "-png", "-aa", "yes"])
            .arg(path)
            .arg(&prefix)
            .status()
            .await
            .context("executando pdftoppm (instale poppler-utils)")?;
        if !status.success() {
            bail!("pdftoppm falhou na página {n}");
        }
        let png = prefix.with_extension("png");
        Ok(tokio::task::spawn_blocking(move || image::open(png).map(|i| i.to_rgba8())).await??)
    }
}

// ---------------------------------------------------------------------------

/// Texto puro → páginas A4 usando cosmic-text (shaping/fallback de fontes do sistema).
pub(crate) mod text {
    use super::*;
    use cosmic_text::{Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache};

    const FONT_PT: f32 = 10.0;
    const LINE_PT: f32 = 13.0;
    const MARGIN_MM: f32 = 15.0;

    fn fonts() -> &'static Mutex<(FontSystem, SwashCache)> {
        static F: OnceLock<Mutex<(FontSystem, SwashCache)>> = OnceLock::new();
        F.get_or_init(|| Mutex::new((FontSystem::new(), SwashCache::new())))
    }

    /// Desenha uma linha de texto (fonte sans do sistema) com topo em `(x, y)`.
    pub fn draw_text(img: &mut RgbaImage, x: i32, y: i32, text: &str, size_px: f32, rgb: [u8; 3]) {
        let mut guard = fonts().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let (fs, cache) = &mut *guard;
        let mut buf = Buffer::new(fs, Metrics::new(size_px, size_px * 1.3));
        buf.set_size(Some((img.width() as i32 - x).max(1) as f32), None);
        buf.set_text(text, &Attrs::new().family(Family::SansSerif), Shaping::Advanced, None);
        let (w, h) = img.dimensions();
        buf.draw(fs, cache, Color::rgb(rgb[0], rgb[1], rgb[2]), |gx, gy, gw, gh, color| {
            for yy in gy..gy + gh as i32 {
                for xx in gx..gx + gw as i32 {
                    let (px, py) = (xx + x, yy + y);
                    if px >= 0 && py >= 0 && (px as u32) < w && (py as u32) < h {
                        blend(img.get_pixel_mut(px as u32, py as u32), color);
                    }
                }
            }
        });
    }

    fn blend(p: &mut image::Rgba<u8>, color: Color) {
        let a = color.a() as u32;
        for (c, src) in [color.r(), color.g(), color.b()].into_iter().enumerate() {
            p.0[c] = ((src as u32 * a + p.0[c] as u32 * (255 - a)) / 255) as u8;
        }
    }

    pub fn render(path: &Path, dpi: f32) -> Result<Vec<RgbaImage>> {
        let content = std::fs::read_to_string(path).context("lendo arquivo de texto")?;
        let s = dpi / 72.0;
        let (pw, ph) = PaperSize::A4.size_pt();
        let (page_w, page_h) = ((pw * s) as u32, (ph * s) as u32);
        let margin = MARGIN_MM * PT_PER_MM * s;
        let line_h = LINE_PT * s;
        // Altura útil múltipla da linha → nenhuma linha cruza a quebra de página.
        let lines_per_page = (((page_h as f32 - 2.0 * margin) / line_h).floor() as usize).max(1);
        let body_h = lines_per_page as f32 * line_h;

        let mut guard = fonts().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let (fs, cache) = &mut *guard;
        let mut buf = Buffer::new(fs, Metrics::new(FONT_PT * s, line_h));
        buf.set_size(Some(page_w as f32 - 2.0 * margin), None);
        buf.set_text(&content.replace('\t', "    "), &Attrs::new().family(Family::Monospace), Shaping::Advanced, None);
        buf.shape_until_scroll(fs, false);

        let total_lines = buf.layout_runs().count().max(1);
        let pages = total_lines.div_ceil(lines_per_page);
        let mut out: Vec<RgbaImage> =
            (0..pages).map(|_| RgbaImage::from_pixel(page_w, page_h, image::Rgba([255, 255, 255, 255]))).collect();

        buf.draw(fs, cache, Color::rgb(0, 0, 0), |x, y, w, h, color| {
            if color.a() == 0 {
                return;
            }
            for yy in (y..y + h as i32).filter(|yy| *yy >= 0) {
                let page = (yy as f32 / body_h) as usize;
                let Some(img) = out.get_mut(page) else { continue };
                let py = (yy as f32 - page as f32 * body_h + margin) as u32;
                for xx in (x..x + w as i32).filter(|xx| *xx >= 0) {
                    let px = (xx as f32 + margin) as u32;
                    if px < page_w && py < page_h {
                        blend(img.get_pixel_mut(px, py), color);
                    }
                }
            }
        });
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_kind_by_extension() {
        assert_eq!(kind_for(Path::new("a.PDF")), Some(Kind::Pdf));
        assert_eq!(kind_for(Path::new("foto.jpeg")), Some(Kind::Image));
        assert_eq!(kind_for(Path::new("x.webp")), Some(Kind::Image));
        assert_eq!(kind_for(Path::new("notas.txt")), Some(Kind::Text));
        assert_eq!(kind_for(Path::new("planilha.ods")), None);
        assert_eq!(kind_for(Path::new("sem_extensao")), None);
    }

    #[test]
    fn parses_pdfinfo_page_sizes() {
        let out = "Pages:           2\n\
                   Page    1 size: 595.276 x 841.89 pts (A4)\n\
                   Page    1 rot:  0\n\
                   Page    2 size: 612 x 792 pts (letter)\n";
        let out: String = out.lines().map(|l| format!("{}\n", l.trim_start())).collect();
        assert_eq!(parse_pdfinfo_sizes(&out), vec![(595.276, 841.89), (612.0, 792.0)]);
        assert!(parse_pdfinfo_sizes("Title: x\n").is_empty());
    }

    #[tokio::test]
    async fn opens_image_with_physical_size_at_96_dpi() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("img.png");
        RgbaImage::from_pixel(192, 96, image::Rgba([0, 0, 255, 255])).save(&path).unwrap();

        let doc = open(path).await.unwrap();
        assert_eq!(doc.kind, Kind::Image);
        assert_eq!(doc.page_sizes, vec![(144.0, 72.0)]); // 2 × 1 polegada
        assert_eq!(doc.title(), "img.png");

        let pages = render_pages(doc, vec![0], 72.0).await.unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].image.get_pixel(10, 10).0, [0, 0, 255, 255]);
    }

    #[tokio::test]
    async fn rejects_unsupported_files() {
        assert!(open(PathBuf::from("/tmp/x.docx")).await.is_err());
    }

    #[tokio::test]
    async fn many_images_become_one_document() {
        let dir = tempfile::tempdir().unwrap();
        let mut paths = vec![];
        for (i, c) in [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]].into_iter().enumerate() {
            let p = dir.path().join(format!("f{i}.png"));
            RgbaImage::from_pixel(96, 48, image::Rgba(c)).save(&p).unwrap();
            paths.push(p);
        }
        let docs = open_many(paths).await.unwrap();
        assert_eq!(docs.len(), 1);
        let doc = &docs[0];
        assert_eq!(doc.page_count(), 3);
        assert_eq!(doc.title(), "f0.png + 2 imagens");
        assert_eq!(doc.page_label(2), "f2.png");

        let pages = render_pages(doc.clone(), vec![2, 0, 2], 72.0).await.unwrap();
        assert_eq!(pages.iter().map(|p| p.index).collect::<Vec<_>>(), vec![2, 0, 2]);
        assert_eq!(pages[0].image.get_pixel(1, 1).0, [0, 0, 255, 255]);
        assert_eq!(pages[1].image.get_pixel(1, 1).0, [255, 0, 0, 255]);
    }

    #[tokio::test]
    async fn mixed_files_stay_separate_and_text_cannot_join_images() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("a.png");
        RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 0, 255])).save(&png).unwrap();
        let txt = dir.path().join("b.txt");
        std::fs::write(&txt, "oi").unwrap();
        let mut docs = open_many(vec![png, txt]).await.unwrap();
        assert_eq!(docs.len(), 2);
        let text = docs.pop().unwrap();
        assert!(docs[0].append_images(text).is_err());
    }

    #[test]
    fn image_physical_size() {
        assert_eq!(image_size_pt(150, 300, 150.0), (72.0, 144.0));
        assert_eq!(image_raster_limit(72.0), 1262);
    }

    #[test]
    fn text_paginates_into_a4_pages() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.txt");
        std::fs::write(&path, "uma linha\n".repeat(150)).unwrap();
        let pages = text::render(&path, 36.0).unwrap();
        assert!(pages.len() >= 2, "150 linhas não cabem numa A4");
        assert_eq!(pages[0].dimensions(), (297, 420));
        // Página tem tinta (texto preto) e margem branca.
        assert!(pages[0].pixels().any(|p| p.0[0] < 128));
        assert_eq!(pages[0].get_pixel(0, 0).0, [255, 255, 255, 255]);
    }
}
