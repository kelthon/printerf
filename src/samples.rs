//! Documentos de amostra para testar a impressora sem procurar arquivos:
//!
//! * [`random_photo`] — foto aleatória de <https://picsum.photos> (Lorem Picsum),
//!   na proporção da folha, para avaliar cor e qualidade fotográfica.
//! * [`test_page`] — página de diagnóstico gerada localmente (offline): barras
//!   CMYK/RGB, rampa de cinza, gradientes, linhas finas, círculos e régua em mm.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use image::{Rgba, RgbaImage};

use crate::document::{self, Document, text::draw_text};
use crate::layout::{Orientation, PT_PER_MM, PrintSettings};

pub const PICSUM_BASE: &str = "https://picsum.photos";
/// DPI usado para dar tamanho físico às fotos do picsum: os presets viram
/// medidas de foto conhecidas (Média = 15 × 10 cm).
pub const PHOTO_DPI: f32 = 150.0;
/// Limite de lado do Lorem Picsum.
pub const PICSUM_MAX_SIDE: u32 = 5000;
pub const MAX_PHOTOS: u8 = 16;
const MAX_PHOTO_BYTES: u64 = 20 * 1024 * 1024;

/// Diretório de cache do app (`$XDG_CACHE_HOME/printerf`).
pub fn cache_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
        .join("printerf")
}

/// Tamanho da foto aleatória. Os presets seguem a proporção de [`PhotoShape`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PhotoSize {
    /// ~8 × 5 cm
    Small,
    /// ~15 × 10 cm (foto 10×15 clássica)
    #[default]
    Medium,
    /// ~25 × 17 cm
    Large,
    /// Proporção e tamanho da folha inteira.
    FullPage,
    /// Largura × altura em pixels (1–5000).
    Custom { width: u32, height: u32 },
}

impl PhotoSize {
    pub const PRESETS: [PhotoSize; 5] = [
        PhotoSize::Small,
        PhotoSize::Medium,
        PhotoSize::Large,
        PhotoSize::FullPage,
        PhotoSize::Custom { width: 1200, height: 800 },
    ];

    /// Lado maior (px) dos presets proporcionais.
    fn long_side(self) -> Option<u32> {
        match self {
            PhotoSize::Small => Some(480),
            PhotoSize::Medium => Some(900),
            PhotoSize::Large => Some(1500),
            PhotoSize::FullPage | PhotoSize::Custom { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PhotoShape {
    /// 3:2
    #[default]
    Landscape,
    /// 2:3
    Portrait,
    /// 1:1
    Square,
}

impl PhotoShape {
    pub const ALL: [PhotoShape; 3] = [PhotoShape::Landscape, PhotoShape::Portrait, PhotoShape::Square];
}

/// Opções do Lorem Picsum (todas opcionais; o padrão é 1 foto média colorida).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PicsumOptions {
    pub size: PhotoSize,
    pub shape: PhotoShape,
    pub grayscale: bool,
    /// 0 = sem desfoque; 1–10 = `?blur=N`.
    pub blur: u8,
    /// Quantas fotos diferentes baixar (1–16) — combine com N-up.
    pub count: u8,
    /// Foto específica (`/id/{id}`) em vez de aleatória.
    pub id: Option<u32>,
}

impl PicsumOptions {
    pub fn count(&self) -> u8 {
        self.count.clamp(1, MAX_PHOTOS)
    }

    /// Dimensões em pixels a pedir ao picsum.
    pub fn dimensions(&self, settings: &PrintSettings) -> (u32, u32) {
        let clamp = |v: u32| v.clamp(1, PICSUM_MAX_SIDE);
        match self.size {
            PhotoSize::Custom { width, height } => (clamp(width), clamp(height)),
            PhotoSize::FullPage => {
                let (w_pt, h_pt) = settings.sheet_pt();
                ((w_pt * PHOTO_DPI / 72.0).round() as u32, (h_pt * PHOTO_DPI / 72.0).round() as u32)
            }
            preset => {
                let long = preset.long_side().unwrap_or(900);
                let short = long * 2 / 3;
                match self.shape {
                    PhotoShape::Landscape => (long, short),
                    PhotoShape::Portrait => (short, long),
                    PhotoShape::Square => (short, short),
                }
            }
        }
    }

    /// Tamanho impresso em centímetros a [`PHOTO_DPI`].
    pub fn print_size_cm(&self, settings: &PrintSettings) -> (f32, f32) {
        let (w, h) = self.dimensions(settings);
        (w as f32 / PHOTO_DPI * 2.54, h as f32 / PHOTO_DPI * 2.54)
    }

    /// URL do Lorem Picsum. `seed` evita cache do CDN (ignorado com `id`).
    pub fn url(&self, settings: &PrintSettings, seed: u64) -> String {
        let (w, h) = self.dimensions(settings);
        let mut query = vec![];
        let path = match self.id {
            Some(id) => format!("{PICSUM_BASE}/id/{id}/{w}/{h}.jpg"),
            None => {
                query.push(format!("random={seed}"));
                format!("{PICSUM_BASE}/{w}/{h}.jpg")
            }
        };
        if self.grayscale {
            query.push("grayscale".into());
        }
        if self.blur > 0 {
            query.push(format!("blur={}", self.blur.min(10)));
        }
        if query.is_empty() { path } else { format!("{path}?{}", query.join("&")) }
    }
}

/// Baixa uma imagem do picsum (bloqueante) e devolve bytes + `picsum-id`.
fn download(url: &str) -> Result<(Vec<u8>, Option<String>)> {
    let mut resp = ureq::get(url).call().with_context(|| format!("baixando {url}"))?;
    let id = resp.headers().get("picsum-id").and_then(|v| v.to_str().ok()).map(str::to_owned);
    let bytes = resp.body_mut().with_config().limit(MAX_PHOTO_BYTES).read_to_vec()?;
    // O CDN pode devolver HTML de erro.
    if image::guess_format(&bytes).is_err() {
        bail!("picsum.photos não devolveu uma imagem");
    }
    Ok((bytes, id))
}

/// Baixa `count` fotos (em paralelo) e as abre como **um** documento de imagens,
/// com tamanho físico a [`PHOTO_DPI`].
pub async fn random_photos(settings: &PrintSettings, opts: &PicsumOptions) -> Result<Document> {
    let base_seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis() as u64;
    let dir = cache_dir();
    tokio::fs::create_dir_all(&dir).await?;

    let mut tasks = tokio::task::JoinSet::new();
    for i in 0..opts.count() as u64 {
        let seed = base_seed + i;
        let url = opts.url(settings, seed);
        let path = dir.join(format!("picsum-{seed}.jpg"));
        tasks.spawn_blocking(move || -> Result<(u64, PathBuf)> {
            let (bytes, id) = download(&url)?;
            std::fs::write(&path, bytes)?;
            tracing::info!("foto picsum #{} salva em {}", id.as_deref().unwrap_or("?"), path.display());
            Ok((i, path))
        });
    }
    let mut paths = Vec::new();
    while let Some(r) = tasks.join_next().await {
        paths.push(r??);
    }
    paths.sort_by_key(|(i, _)| *i);

    let mut docs = paths.into_iter().map(|(_, p)| document::open_image_at(p, PHOTO_DPI));
    let mut doc = docs.next().context("nenhuma foto baixada")??;
    for d in docs {
        doc.append_images(d?)?;
    }
    Ok(doc)
}

/// Gera a página de teste (PNG a 300 DPI) e devolve um documento com o tamanho
/// físico exato da folha — então “tamanho original” imprime a régua em escala 1:1.
pub async fn test_page(settings: &PrintSettings, printer: Option<String>) -> Result<Document> {
    let s = settings.clone();
    let path = cache_dir().join("pagina-de-teste.png");
    let p = path.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        std::fs::create_dir_all(p.parent().context("caminho sem diretório")?)?;
        render_test_page(&s, printer.as_deref(), 300.0).save(&p)?;
        Ok(())
    })
    .await??;
    Ok(Document::image(path, settings.sheet_pt()))
}

// ---------------------------------------------------------------------------

const BLACK: Rgba<u8> = Rgba([0, 0, 0, 255]);

fn fill_rect(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, c: Rgba<u8>) {
    let (iw, ih) = img.dimensions();
    for yy in y..(y + h).min(ih) {
        for xx in x..(x + w).min(iw) {
            img.put_pixel(xx, yy, c);
        }
    }
}

fn stroke_rect(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, t: u32, c: Rgba<u8>) {
    fill_rect(img, x, y, w, t, c);
    fill_rect(img, x, y + h - t, w, t, c);
    fill_rect(img, x, y, t, h, c);
    fill_rect(img, x + w - t, y, t, h, c);
}

/// HSV (h em 0..1, s = v = 1) → RGB.
fn hue(h: f32) -> Rgba<u8> {
    let x = |n: f32| {
        let k = (n + h * 6.0) % 6.0;
        (255.0 * (1.0 - k.min(4.0 - k).clamp(0.0, 1.0))) as u8
    };
    Rgba([x(5.0), x(3.0), x(1.0), 255])
}

/// `AAAA-MM-DD HH:MM UTC` sem depender de crate de datas (algoritmo civil de H. Hinnant).
pub fn utc_timestamp(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let (hh, mm) = ((secs % 86_400) / 3600, (secs % 3600) / 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02} UTC")
}

pub fn render_test_page(settings: &PrintSettings, printer: Option<&str>, dpi: f32) -> RgbaImage {
    let px = |mm: f32| (mm * PT_PER_MM * dpi / 72.0).round() as u32;
    let (w_pt, h_pt) = settings.sheet_pt();
    let (w, h) = ((w_pt * dpi / 72.0).round() as u32, (h_pt * dpi / 72.0).round() as u32);
    let mut img = RgbaImage::from_pixel(w, h, Rgba([255, 255, 255, 255]));
    let m = px(10.0);
    let inner_w = w - 2 * m;

    // Moldura + marcas de registro nos cantos
    stroke_rect(&mut img, m, m, inner_w, h - 2 * m, px(0.5).max(1), BLACK);
    for (cx, cy) in [(m, m), (w - m, m), (m, h - m), (w - m, h - m)] {
        let arm = px(4.0);
        fill_rect(&mut img, cx.saturating_sub(arm), cy, 2 * arm, 2, BLACK);
        fill_rect(&mut img, cx, cy.saturating_sub(arm), 2, 2 * arm, BLACK);
    }

    // Régua em mm (topo e lateral)
    let ruler_len = (inner_w - px(10.0)) as f32 / dpi * 25.4;
    for mm in 0..=(ruler_len as u32).min(180) {
        let len = if mm % 10 == 0 {
            px(4.0)
        } else if mm % 5 == 0 {
            px(2.5)
        } else {
            px(1.5)
        };
        fill_rect(&mut img, m + px(5.0) + px(mm as f32), m, 2, len, BLACK);
        fill_rect(&mut img, m, m + px(5.0) + px(mm as f32), len, 2, BLACK);
    }

    let mut y = m + px(10.0);
    let x0 = m + px(10.0);
    let content_w = inner_w - px(20.0);
    let size = |mm: f32| px(mm) as f32;

    draw_text(&mut img, x0 as i32, y as i32, "PrinterF — página de teste", size(8.0), [0, 0, 0]);
    y += px(12.0);
    let when = utc_timestamp(
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
    );
    let info = format!(
        "Impressora: {} · Papel: {} · {} · {dpi:.0} DPI · gerada em {when}",
        printer.unwrap_or("(escolhida no diálogo do sistema)"),
        settings.paper.cups_media(),
        if settings.orientation == Orientation::Landscape { "paisagem" } else { "retrato" },
    );
    draw_text(&mut img, x0 as i32, y as i32, &info, size(3.2), [60, 60, 60]);
    y += px(10.0);

    // Swatches CMYK + RGB: cada tanque da Epson deve sair limpo, sem faixas.
    let swatches: [(&str, [u8; 3]); 7] = [
        ("C", [0, 174, 239]),
        ("M", [236, 0, 140]),
        ("Y", [255, 242, 0]),
        ("K", [0, 0, 0]),
        ("R", [255, 0, 0]),
        ("G", [0, 166, 81]),
        ("B", [0, 84, 166]),
    ];
    let gap = px(3.0);
    let sw = (content_w - gap * (swatches.len() as u32 - 1)) / swatches.len() as u32;
    for (i, (label, [r, g, b])) in swatches.iter().enumerate() {
        let x = x0 + i as u32 * (sw + gap);
        fill_rect(&mut img, x, y, sw, px(20.0), Rgba([*r, *g, *b, 255]));
        draw_text(&mut img, x as i32, (y + px(21.0)) as i32, label, size(3.5), [0, 0, 0]);
    }
    y += px(30.0);

    // Rampa de cinza em 11 passos (0–100 %): todos os passos devem ser distinguíveis.
    let steps = 11;
    let gw = content_w / steps;
    for i in 0..steps {
        let v = 255 - (i * 255 / (steps - 1)) as u8;
        fill_rect(&mut img, x0 + i * gw, y, gw, px(12.0), Rgba([v, v, v, 255]));
        draw_text(&mut img, (x0 + i * gw) as i32, (y + px(13.0)) as i32, &format!("{}%", i * 10), size(2.6), [0, 0, 0]);
    }
    stroke_rect(&mut img, x0, y, gw * steps, px(12.0), 1, BLACK);
    y += px(22.0);

    // Gradientes contínuos (detecta banding/dithering ruim)
    for xx in 0..content_w {
        let t = xx as f32 / content_w as f32;
        let c = hue(t);
        let g = (255.0 * (1.0 - t)) as u8;
        fill_rect(&mut img, x0 + xx, y, 1, px(10.0), c);
        fill_rect(&mut img, x0 + xx, y + px(12.0), 1, px(10.0), Rgba([g, g, g, 255]));
    }
    y += px(30.0);

    // Linhas finas: 1, 2 e 3 px a 300 DPI (≈0,08/0,17/0,25 mm) — testa bicos entupidos.
    let block = (content_w - 2 * gap) / 3;
    for (i, t) in [1u32, 2, 3].into_iter().enumerate() {
        let bx = x0 + i as u32 * (block + gap);
        let half = block / 2;
        for k in (0..half).step_by((2 * t) as usize) {
            fill_rect(&mut img, bx + k, y, t, px(25.0), BLACK); // verticais
        }
        for k in (0..px(25.0)).step_by((2 * t) as usize) {
            fill_rect(&mut img, bx + half, y + k, half, t, BLACK); // horizontais
        }
        draw_text(&mut img, bx as i32, (y + px(26.0)) as i32, &format!("{t} px"), size(3.0), [0, 0, 0]);
    }
    y += px(36.0);

    // Círculos concêntricos: devem sair redondos (proporção/escala corretas).
    let r_max = ((h - m - px(10.0)).saturating_sub(y) / 2).min(content_w / 2).min(px(45.0));
    let (cx, cy) = (x0 + content_w / 2, y + r_max);
    for r in (px(5.0)..=r_max).step_by(px(5.0) as usize) {
        let steps = (r as f32 * 7.0) as u32;
        for k in 0..steps {
            let a = k as f32 / steps as f32 * std::f32::consts::TAU;
            let (xx, yy) = (cx as f32 + r as f32 * a.cos(), cy as f32 + r as f32 * a.sin());
            fill_rect(&mut img, xx as u32, yy as u32, 3, 3, BLACK);
        }
    }
    if r_max > px(10.0) {
        draw_text(&mut img, (cx + r_max + gap) as i32, cy as i32, "círculos: 5 mm entre anéis", size(3.0), [0, 0, 0]);
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Kind;

    #[test]
    fn picsum_defaults_to_one_medium_color_photo() {
        let o = PicsumOptions::default();
        assert_eq!(o.count(), 1);
        assert_eq!(o.dimensions(&PrintSettings::default()), (900, 600));
        let (w, h) = o.print_size_cm(&PrintSettings::default());
        assert!((w - 15.24).abs() < 0.01 && (h - 10.16).abs() < 0.01, "{w}x{h}");
        assert_eq!(o.url(&PrintSettings::default(), 42), "https://picsum.photos/900/600.jpg?random=42");
    }

    #[test]
    fn picsum_presets_shapes_and_custom() {
        let s = PrintSettings::default();
        let dims = |size, shape| PicsumOptions { size, shape, ..Default::default() }.dimensions(&s);
        assert_eq!(dims(PhotoSize::Small, PhotoShape::Portrait), (320, 480));
        assert_eq!(dims(PhotoSize::Large, PhotoShape::Square), (1000, 1000));
        assert_eq!(dims(PhotoSize::FullPage, PhotoShape::Landscape), (1240, 1754), "folha manda, não o formato");
        assert_eq!(dims(PhotoSize::Custom { width: 0, height: 9000 }, PhotoShape::Square), (1, 5000));
        let land = PrintSettings { orientation: Orientation::Landscape, ..Default::default() };
        assert_eq!(PicsumOptions { size: PhotoSize::FullPage, ..Default::default() }.dimensions(&land), (1754, 1240));
    }

    #[test]
    fn picsum_url_options() {
        let s = PrintSettings::default();
        let o = PicsumOptions { grayscale: true, blur: 3, ..Default::default() };
        assert_eq!(o.url(&s, 7), "https://picsum.photos/900/600.jpg?random=7&grayscale&blur=3");
        let o = PicsumOptions { id: Some(237), blur: 99, ..Default::default() };
        assert_eq!(o.url(&s, 7), "https://picsum.photos/id/237/900/600.jpg?blur=10");
        assert_eq!(PicsumOptions { count: 40, ..Default::default() }.count(), MAX_PHOTOS);
        assert_eq!(PicsumOptions { count: 0, ..Default::default() }.count(), 1);
    }

    #[test]
    fn formats_utc_timestamps() {
        assert_eq!(utc_timestamp(0), "1970-01-01 00:00 UTC");
        assert_eq!(utc_timestamp(951_782_400), "2000-02-29 00:00 UTC");
        assert_eq!(utc_timestamp(1_792_262_745), "2026-10-17 18:45 UTC");
    }

    #[test]
    fn test_page_has_expected_size_and_colors() {
        let img = render_test_page(&PrintSettings::default(), Some("EPSON_L3250"), 72.0);
        assert_eq!(img.dimensions(), (595, 842));
        let has = |c: [u8; 3]| img.pixels().any(|p| p.0[..3] == c);
        assert!(has([0, 174, 239]), "ciano");
        assert!(has([236, 0, 140]), "magenta");
        assert!(has([255, 242, 0]), "amarelo");
        assert!(has([0, 0, 0]), "preto");
        assert_eq!(img.get_pixel(2, 2).0, [255, 255, 255, 255], "fora da moldura fica branco");
    }

    #[tokio::test]
    async fn test_page_document_is_sheet_sized() {
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: teste isolado; nenhum outro teste lê XDG_CACHE_HOME.
        unsafe { std::env::set_var("XDG_CACHE_HOME", dir.path()) };
        let doc = test_page(&PrintSettings::default(), None).await.unwrap();
        assert_eq!(doc.kind, Kind::Image);
        assert_eq!(doc.page_sizes, vec![PrintSettings::default().sheet_pt()]);
        assert_eq!(image::image_dimensions(&doc.path).unwrap(), (2480, 3508));
    }

    /// Acessa a internet; rode com `cargo test -- --ignored picsum`.
    #[tokio::test]
    #[ignore]
    async fn picsum_download_works() {
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: teste isolado (rodado sozinho via --ignored).
        unsafe { std::env::set_var("XDG_CACHE_HOME", dir.path()) };
        let opts =
            PicsumOptions { size: PhotoSize::Small, shape: PhotoShape::Portrait, count: 2, ..Default::default() };
        let doc = random_photos(&PrintSettings::default(), &opts).await.unwrap();
        assert_eq!(doc.kind, Kind::Image);
        assert_eq!(doc.page_count(), 2);
        assert_eq!(image::image_dimensions(&doc.images[1]).unwrap(), (320, 480));
        // 320 px a 150 DPI = 153,6 pt
        assert!((doc.page_sizes[0].0 - 153.6).abs() < 0.1);
    }
}
