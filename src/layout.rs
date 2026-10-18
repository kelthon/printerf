//! Modelo de configurações de impressão e composição de folhas (puro, sem I/O).
//!
//! Tudo aqui é determinístico e testável: dado um conjunto de páginas-fonte e
//! [`PrintSettings`], [`compose_sheet`] produz o raster exato de uma folha. O mesmo
//! código gera o preview (DPI baixo) e o spool (300 DPI) → o preview é WYSIWYG.

use std::collections::BTreeMap;

use image::{RgbaImage, imageops};

use crate::cups::JobOptions;

pub const PT_PER_MM: f32 = 72.0 / 25.4;
/// Margem não imprimível conservadora para jato de tinta Epson.
pub const MARGIN_MM: f32 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaperSize {
    #[default]
    A4,
    Letter,
}

impl PaperSize {
    pub const ALL: [PaperSize; 2] = [PaperSize::A4, PaperSize::Letter];
    /// Tamanho em pontos (retrato).
    pub fn size_pt(self) -> (f32, f32) {
        match self {
            PaperSize::A4 => (595.28, 841.89),
            PaperSize::Letter => (612.0, 792.0),
        }
    }
    pub fn cups_media(self) -> &'static str {
        match self {
            PaperSize::A4 => "A4",
            PaperSize::Letter => "Letter",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Orientation {
    #[default]
    Portrait,
    Landscape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Rotation {
    #[default]
    R0,
    R90,
    R180,
    R270,
}

impl Rotation {
    pub const ALL: [Rotation; 4] = [Rotation::R0, Rotation::R90, Rotation::R180, Rotation::R270];
    fn swaps_axes(self) -> bool {
        matches!(self, Rotation::R90 | Rotation::R270)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FitMode {
    /// Cabe inteiro na célula, preservando proporção (pode sobrar margem).
    #[default]
    Fit,
    /// Preenche a célula, preservando proporção (corta o excesso).
    Fill,
    /// Tamanho físico original (100%), centralizado; corta se não couber.
    Original,
}

impl FitMode {
    pub const ALL: [FitMode; 3] = [FitMode::Fit, FitMode::Fill, FitMode::Original];
}

pub const NUP_CHOICES: [u8; 8] = [1, 2, 4, 6, 8, 9, 12, 16];

/// Corte em frações (0–1) de cada borda da imagem já rotacionada.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Crop {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Crop {
    /// Máximo cortado por borda: sempre sobram ≥ 10 % da imagem.
    pub const MAX: f32 = 0.45;

    pub fn clamped(self) -> Self {
        let c = |v: f32| v.clamp(0.0, Self::MAX);
        Self { left: c(self.left), top: c(self.top), right: c(self.right), bottom: c(self.bottom) }
    }
}

/// Ajuste livre de uma imagem na sua célula: corte → escala do modo de ajuste ×
/// `zoom` → centralizada + `offset` (fração da célula). Identidade = `Default`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItemTransform {
    pub crop: Crop,
    pub zoom: f32,
    pub offset_x: f32,
    pub offset_y: f32,
}

impl Default for ItemTransform {
    fn default() -> Self {
        Self { crop: Crop::default(), zoom: 1.0, offset_x: 0.0, offset_y: 0.0 }
    }
}

impl ItemTransform {
    pub const ZOOM_MIN: f32 = 0.1;
    pub const ZOOM_MAX: f32 = 4.0;

    pub fn is_identity(&self) -> bool {
        *self == Self::default()
    }

    pub fn with_zoom(mut self, zoom: f32) -> Self {
        self.zoom = zoom.clamp(Self::ZOOM_MIN, Self::ZOOM_MAX);
        self
    }

    /// Desloca; limita para a imagem não sair inteira da célula.
    pub fn moved(mut self, dx: f32, dy: f32) -> Self {
        self.offset_x = (self.offset_x + dx).clamp(-1.0, 1.0);
        self.offset_y = (self.offset_y + dy).clamp(-1.0, 1.0);
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrintSettings {
    pub paper: PaperSize,
    pub orientation: Orientation,
    pub rotation: Rotation,
    pub fit: FitMode,
    pub nup: u8,
    pub copies: u16,
    pub duplex: bool,
    /// Valor de `media-source` (bandeja). `None` = automático.
    pub tray: Option<String>,
    /// Texto livre do usuário, ex.: `1-3, 5, 8`. Vazio = todas.
    pub page_range: String,
    /// Só PDF: envia o PDF original e deixa os filtros do CUPS fazerem N-up/escala.
    pub vector_passthrough: bool,
    /// Repete cada página/imagem em todas as células da folha (folha de fotos).
    pub repeat: bool,
    /// Ajustes livres por página-fonte (índice 0-based no documento).
    pub transforms: BTreeMap<usize, ItemTransform>,
}

impl Default for PrintSettings {
    fn default() -> Self {
        Self {
            paper: PaperSize::A4,
            orientation: Orientation::Portrait,
            rotation: Rotation::R0,
            fit: FitMode::Fit,
            nup: 1,
            copies: 1,
            duplex: false,
            tray: None,
            page_range: String::new(),
            vector_passthrough: false,
            repeat: false,
            transforms: BTreeMap::new(),
        }
    }
}

impl PrintSettings {
    /// Tamanho da folha em pontos já considerando orientação.
    pub fn sheet_pt(&self) -> (f32, f32) {
        let (w, h) = self.paper.size_pt();
        match self.orientation {
            Orientation::Portrait => (w, h),
            Orientation::Landscape => (h, w),
        }
    }

    pub fn transform(&self, page: usize) -> ItemTransform {
        self.transforms.get(&page).copied().unwrap_or_default()
    }

    /// Grava o ajuste; identidade remove a entrada (mantém `PartialEq` estável).
    pub fn set_transform(&mut self, page: usize, t: ItemTransform) {
        if t.is_identity() {
            self.transforms.remove(&page);
        } else {
            self.transforms.insert(page, t);
        }
    }

    /// Ordem de impressão: intervalo aplicado e, com `repeat`, cada página
    /// repetida `nup` vezes (uma folha cheia por página).
    pub fn print_order(&self, total_pages: usize) -> Result<Vec<usize>, String> {
        let pages = parse_page_range(&self.page_range, total_pages)?;
        Ok(if self.repeat && self.nup > 1 {
            pages.into_iter().flat_map(|p| std::iter::repeat_n(p, self.nup as usize)).collect()
        } else {
            pages
        })
    }

    /// Opções CUPS comuns aos dois modos de spool.
    fn base_options(&self) -> JobOptions {
        let mut o = JobOptions::default();
        o.set("media", self.paper.cups_media()).set("copies", self.copies.to_string());
        o.set(
            "sides",
            match (self.duplex, self.orientation) {
                (false, _) => "one-sided",
                (true, Orientation::Portrait) => "two-sided-long-edge",
                (true, Orientation::Landscape) => "two-sided-short-edge",
            },
        );
        if let Some(tray) = &self.tray {
            o.set("media-source", tray.clone());
        }
        if self.copies > 1 {
            o.set("collate", "true");
        }
        o
    }

    /// Opções para um spool que **já** foi composto por nós (raster → PDF):
    /// rotação, N-up, escala e intervalo já estão “queimados” nas páginas.
    pub fn cups_options_composed(&self) -> JobOptions {
        let mut o = self.base_options();
        o.set("print-scaling", "none");
        o
    }

    /// Opções para pass-through vetorial: o cups-filters (pdftopdf) aplica tudo.
    pub fn cups_options_passthrough(&self) -> JobOptions {
        let mut o = self.base_options();
        if !self.page_range.trim().is_empty() {
            o.set("page-ranges", self.page_range.replace(' ', ""));
        }
        o.set("number-up", self.nup.to_string());
        o.set(
            "orientation-requested",
            match (self.orientation, self.rotation) {
                (Orientation::Portrait, Rotation::R0 | Rotation::R180) => "3",
                (Orientation::Landscape, Rotation::R0 | Rotation::R180) => "4",
                (Orientation::Portrait, _) => "4",
                (Orientation::Landscape, _) => "3",
            },
        );
        o.set(
            "print-scaling",
            match self.fit {
                FitMode::Fit => "fit",
                FitMode::Fill => "fill",
                FitMode::Original => "none",
            },
        );
        o
    }
}

// ---------------------------------------------------------------------------
// Intervalo de páginas
// ---------------------------------------------------------------------------

/// Converte `"1-3, 5, 8, 10-"` em índices **0-based**, na ordem dada.
/// Vazio → todas as páginas.
pub fn parse_page_range(input: &str, total: usize) -> Result<Vec<usize>, String> {
    let input = input.trim();
    if input.is_empty() {
        return Ok((0..total).collect());
    }
    let mut out = Vec::new();
    for part in input.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let num = |s: &str| -> Result<usize, String> {
            let n: usize = s.trim().parse().map_err(|_| format!("“{s}” não é um número"))?;
            if n == 0 || n > total {
                return Err(format!("página {n} fora do intervalo 1–{total}"));
            }
            Ok(n)
        };
        match part.split_once('-') {
            Some((a, b)) => {
                let start = if a.trim().is_empty() { 1 } else { num(a)? };
                let end = if b.trim().is_empty() { total } else { num(b)? };
                if start > end {
                    return Err(format!("intervalo invertido: {part}"));
                }
                out.extend((start..=end).map(|n| n - 1));
            }
            None => out.push(num(part)? - 1),
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Composição
// ---------------------------------------------------------------------------

/// Uma página-fonte rasterizada + seu tamanho físico (para `FitMode::Original`).
#[derive(Debug, Clone)]
pub struct SourcePage {
    /// Índice da página no documento (chave de [`PrintSettings::transforms`]).
    pub index: usize,
    pub image: RgbaImage,
    pub width_pt: f32,
    pub height_pt: f32,
}

/// Grade (colunas, linhas) do N-up, escolhida para casar com a orientação da folha.
pub fn nup_grid(n: u8, landscape: bool) -> (u32, u32) {
    let (a, b) = match n {
        2 => (1, 2),
        4 => (2, 2),
        6 => (2, 3),
        8 => (2, 4),
        9 => (3, 3),
        12 => (3, 4),
        16 => (4, 4),
        _ => (1, 1),
    };
    if landscape { (b, a) } else { (a, b) }
}

pub fn sheet_count(pages: usize, nup: u8) -> usize {
    pages.div_ceil(nup.max(1) as usize)
}

/// Geometria da folha em pixels a um DPI: margens, grade e células.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SheetGeometry {
    pub width: u32,
    pub height: u32,
    pub margin: u32,
    pub gutter: u32,
    pub cols: u32,
    pub rows: u32,
    pub cell_w: u32,
    pub cell_h: u32,
}

impl SheetGeometry {
    pub fn new(settings: &PrintSettings, dpi: f32) -> Self {
        let px = |pt: f32| (pt * dpi / 72.0).round().max(1.0) as u32;
        let (sw_pt, sh_pt) = settings.sheet_pt();
        let (width, height) = (px(sw_pt), px(sh_pt));
        let margin = px(MARGIN_MM * PT_PER_MM);
        let gutter = if settings.nup > 1 { px(3.0 * PT_PER_MM) } else { 0 };
        let (cols, rows) = nup_grid(settings.nup, width > height);
        let cell_w = (width.saturating_sub(2 * margin).saturating_sub(gutter * (cols - 1))) / cols;
        let cell_h = (height.saturating_sub(2 * margin).saturating_sub(gutter * (rows - 1))) / rows;
        Self { width, height, margin, gutter, cols, rows, cell_w, cell_h }
    }

    /// Canto superior esquerdo da célula `slot`.
    pub fn cell_origin(&self, slot: usize) -> (u32, u32) {
        let (col, row) = (slot as u32 % self.cols, slot as u32 / self.cols);
        (self.margin + col * (self.cell_w + self.gutter), self.margin + row * (self.cell_h + self.gutter))
    }

    /// Célula sob o ponto `(fx, fy)` em frações da folha (0–1), se houver.
    pub fn slot_at(&self, fx: f32, fy: f32) -> Option<usize> {
        let (x, y) = (fx * self.width as f32, fy * self.height as f32);
        (0..(self.cols * self.rows) as usize).find(|&slot| {
            let (cx, cy) = self.cell_origin(slot);
            x >= cx as f32 && x < (cx + self.cell_w) as f32 && y >= cy as f32 && y < (cy + self.cell_h) as f32
        })
    }

    /// Converte um deslocamento em frações da folha para frações da célula.
    pub fn sheet_to_cell_delta(&self, dx: f32, dy: f32) -> (f32, f32) {
        (dx * self.width as f32 / self.cell_w.max(1) as f32, dy * self.height as f32 / self.cell_h.max(1) as f32)
    }
}

fn rotate(img: &RgbaImage, r: Rotation) -> RgbaImage {
    match r {
        Rotation::R0 => img.clone(),
        Rotation::R90 => imageops::rotate90(img),
        Rotation::R180 => imageops::rotate180(img),
        Rotation::R270 => imageops::rotate270(img),
    }
}

/// Compõe a folha `sheet_index` a `dpi`. `pages` estão na ordem de impressão
/// (veja [`PrintSettings::print_order`]).
pub fn compose_sheet(pages: &[SourcePage], settings: &PrintSettings, sheet_index: usize, dpi: f32) -> RgbaImage {
    let px = |pt: f32| (pt * dpi / 72.0).round().max(1.0) as u32;
    let g = SheetGeometry::new(settings, dpi);
    let mut sheet = RgbaImage::from_pixel(g.width, g.height, image::Rgba([255, 255, 255, 255]));

    let per = settings.nup.max(1) as usize;
    for (slot, page) in pages.iter().skip(sheet_index * per).take(per).enumerate() {
        let (cx, cy) = g.cell_origin(slot);
        let t = settings.transform(page.index);
        let crop = t.crop.clamped();

        // 1. rotação e corte (frações da imagem rotacionada)
        let rotated = rotate(&page.image, settings.rotation);
        let (rw, rh) = rotated.dimensions();
        let kx = 1.0 - crop.left - crop.right;
        let ky = 1.0 - crop.top - crop.bottom;
        let (sx0, sy0) = ((crop.left * rw as f32) as u32, (crop.top * rh as f32) as u32);
        let (iw, ih) = (((rw as f32 * kx) as u32).max(1), ((rh as f32 * ky) as u32).max(1));
        let (pw, ph) = if settings.rotation.swaps_axes() {
            (page.height_pt * kx, page.width_pt * ky)
        } else {
            (page.width_pt * kx, page.height_pt * ky)
        };

        // 2. tamanho-alvo: modo de ajuste × zoom
        let (iwf, ihf) = (iw as f32, ih as f32);
        let (tw, th) = match settings.fit {
            FitMode::Fit => {
                let s = (g.cell_w as f32 / iwf).min(g.cell_h as f32 / ihf);
                (iwf * s, ihf * s)
            }
            FitMode::Fill => {
                let s = (g.cell_w as f32 / iwf).max(g.cell_h as f32 / ihf);
                (iwf * s, ihf * s)
            }
            // Tamanho físico real, também em N-up (fotos 15×10 cm saem com 15×10 cm).
            FitMode::Original => (px(pw) as f32, px(ph) as f32),
        };
        let (tw, th) = (tw * t.zoom, th * t.zoom);

        // 3. posição na célula (centro + offset) e recorte ao que é visível
        let ox = (g.cell_w as f32 - tw) / 2.0 + t.offset_x * g.cell_w as f32;
        let oy = (g.cell_h as f32 - th) / 2.0 + t.offset_y * g.cell_h as f32;
        let (vx0, vy0) = (ox.max(0.0), oy.max(0.0));
        let (vx1, vy1) = ((ox + tw).min(g.cell_w as f32), (oy + th).min(g.cell_h as f32));
        if vx1 - vx0 < 1.0 || vy1 - vy0 < 1.0 {
            continue; // fora da célula
        }
        // Só a região visível da fonte é reamostrada: zoom alto não estoura memória.
        let (scale_x, scale_y) = (tw / iwf, th / ihf);
        let src_x = sx0 + ((vx0 - ox) / scale_x) as u32;
        let src_y = sy0 + ((vy0 - oy) / scale_y) as u32;
        // Limita à região cortada (não à imagem inteira), senão o `ceil` vaza 1 px do corte.
        let (src_x, src_y) = (src_x.min(sx0 + iw - 1), src_y.min(sy0 + ih - 1));
        let src_w = (((vx1 - vx0) / scale_x).ceil() as u32).clamp(1, sx0 + iw - src_x);
        let src_h = (((vy1 - vy0) / scale_y).ceil() as u32).clamp(1, sy0 + ih - src_y);
        let region = imageops::crop_imm(&rotated, src_x, src_y, src_w, src_h).to_image();
        let (dw, dh) = ((vx1 - vx0).round() as u32, (vy1 - vy0).round() as u32);
        let visible = imageops::resize(&region, dw.max(1), dh.max(1), imageops::FilterType::Triangle);
        imageops::overlay(&mut sheet, &visible, (cx as f32 + vx0) as i64, (cy as f32 + vy0) as i64);
    }
    sheet
}

/// Desenha o contorno da célula `slot` (realce de seleção no preview).
pub fn outline_slot(sheet: &mut RgbaImage, g: &SheetGeometry, slot: usize, color: [u8; 4]) {
    let (x, y) = g.cell_origin(slot);
    let (w, h) = (g.cell_w, g.cell_h);
    let c = image::Rgba(color);
    for t in 0..2 {
        for xx in x..(x + w).min(sheet.width()) {
            for yy in [y + t, (y + h).saturating_sub(1 + t)] {
                if yy < sheet.height() {
                    sheet.put_pixel(xx, yy, c);
                }
            }
        }
        for yy in y..(y + h).min(sheet.height()) {
            for xx in [x + t, (x + w).saturating_sub(1 + t)] {
                if xx < sheet.width() {
                    sheet.put_pixel(xx, yy, c);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_ranges() {
        assert_eq!(parse_page_range("", 3).unwrap(), vec![0, 1, 2]);
        assert_eq!(parse_page_range("1-3, 5, 8", 10).unwrap(), vec![0, 1, 2, 4, 7]);
        assert_eq!(parse_page_range("9-", 10).unwrap(), vec![8, 9]);
        assert!(parse_page_range("0", 10).is_err());
        assert!(parse_page_range("4-2", 10).is_err());
        assert!(parse_page_range("11", 10).is_err());
        assert!(parse_page_range("a", 10).is_err());
    }

    #[test]
    fn nup_grid_matches_orientation() {
        assert_eq!(nup_grid(2, false), (1, 2));
        assert_eq!(nup_grid(2, true), (2, 1));
        assert_eq!(nup_grid(6, true), (3, 2));
        assert_eq!(sheet_count(5, 4), 2);
    }

    #[test]
    fn compose_places_pages_inside_sheet() {
        let red = SourcePage {
            index: 0,
            image: RgbaImage::from_pixel(100, 50, image::Rgba([255, 0, 0, 255])),
            width_pt: 200.0,
            height_pt: 100.0,
        };
        let s = PrintSettings { nup: 2, ..Default::default() };
        let sheet = compose_sheet(&[red.clone(), red], &s, 0, 36.0);
        let (w, h) = sheet.dimensions();
        assert_eq!((w, h), (298, 421)); // A4 a 36 dpi
        // Centro das duas células (metades superior/inferior) deve estar vermelho.
        assert_eq!(sheet.get_pixel(w / 2, h / 4).0, [255, 0, 0, 255]);
        assert_eq!(sheet.get_pixel(w / 2, 3 * h / 4).0, [255, 0, 0, 255]);
        // Canto (margem) continua branco.
        assert_eq!(sheet.get_pixel(1, 1).0, [255, 255, 255, 255]);
    }

    fn opts(o: &JobOptions) -> std::collections::HashMap<&str, &str> {
        o.0.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect()
    }

    #[test]
    fn page_range_open_start_and_spaces() {
        assert_eq!(parse_page_range(" -2 ,4 ", 5).unwrap(), vec![0, 1, 3]);
        assert_eq!(parse_page_range("1,,2", 5).unwrap(), vec![0, 1]);
    }

    #[test]
    fn landscape_swaps_sheet_size() {
        let s = PrintSettings { orientation: Orientation::Landscape, paper: PaperSize::Letter, ..Default::default() };
        assert_eq!(s.sheet_pt(), (792.0, 612.0));
    }

    #[test]
    fn composed_options_never_rescale() {
        let s = PrintSettings { copies: 3, tray: Some("rear".into()), ..Default::default() };
        let o = s.cups_options_composed();
        let m = opts(&o);
        assert_eq!(m["media"], "A4");
        assert_eq!(m["copies"], "3");
        assert_eq!(m["collate"], "true");
        assert_eq!(m["media-source"], "rear");
        assert_eq!(m["sides"], "one-sided");
        assert_eq!(m["print-scaling"], "none");
        assert!(!m.contains_key("number-up"), "N-up já foi aplicado na composição");
    }

    #[test]
    fn duplex_edge_follows_orientation() {
        let mut s = PrintSettings { duplex: true, ..Default::default() };
        assert_eq!(opts(&s.cups_options_composed())["sides"], "two-sided-long-edge");
        s.orientation = Orientation::Landscape;
        assert_eq!(opts(&s.cups_options_composed())["sides"], "two-sided-short-edge");
    }

    #[test]
    fn passthrough_maps_to_ipp_attributes() {
        let s = PrintSettings {
            nup: 4,
            page_range: "1 - 3, 5".into(),
            fit: FitMode::Fill,
            rotation: Rotation::R90,
            ..Default::default()
        };
        let o = s.cups_options_passthrough();
        let m = opts(&o);
        assert_eq!(m["page-ranges"], "1-3,5");
        assert_eq!(m["number-up"], "4");
        assert_eq!(m["print-scaling"], "fill");
        assert_eq!(m["orientation-requested"], "4"); // retrato + 90° = paisagem
        assert!(!m.contains_key("collate"));
    }

    fn page(w: u32, h: u32, color: [u8; 4], wpt: f32, hpt: f32) -> SourcePage {
        SourcePage { index: 0, image: RgbaImage::from_pixel(w, h, image::Rgba(color)), width_pt: wpt, height_pt: hpt }
    }

    #[test]
    fn fill_covers_whole_printable_area() {
        let s = PrintSettings { fit: FitMode::Fill, ..Default::default() };
        let sheet = compose_sheet(&[page(10, 10, [0, 255, 0, 255], 10.0, 10.0)], &s, 0, 72.0);
        let m = (MARGIN_MM * PT_PER_MM).round() as u32 + 1;
        let (w, h) = sheet.dimensions();
        for (x, y) in [(m, m), (w - m - 1, h - m - 1), (m, h - m - 1)] {
            assert_eq!(sheet.get_pixel(x, y).0, [0, 255, 0, 255], "({x},{y})");
        }
    }

    #[test]
    fn fit_letterboxes_wide_image() {
        let sheet = compose_sheet(&[page(200, 100, [0, 0, 0, 255], 200.0, 100.0)], &PrintSettings::default(), 0, 72.0);
        let (w, h) = sheet.dimensions();
        assert_eq!(sheet.get_pixel(w / 2, h / 2).0, [0, 0, 0, 255]);
        assert_eq!(sheet.get_pixel(w / 2, h / 8).0, [255, 255, 255, 255], "faixa branca acima");
    }

    #[test]
    fn rotation_turns_wide_into_tall() {
        // Imagem 2:1 rotacionada 90° numa folha retrato com Fit → ocupa quase toda a altura.
        let s = PrintSettings { rotation: Rotation::R90, ..Default::default() };
        let sheet = compose_sheet(&[page(200, 100, [0, 0, 0, 255], 200.0, 100.0)], &s, 0, 72.0);
        let (w, h) = sheet.dimensions();
        assert_eq!(sheet.get_pixel(w / 2, h / 8).0, [0, 0, 0, 255]);
    }

    #[test]
    fn original_size_is_physical() {
        // 1 × 1 polegada a 100 dpi → 100 × 100 px pretos centralizados.
        let s = PrintSettings { fit: FitMode::Original, ..Default::default() };
        let sheet = compose_sheet(&[page(7, 7, [0, 0, 0, 255], 72.0, 72.0)], &s, 0, 100.0);
        let black = sheet.pixels().filter(|p| p.0 == [0, 0, 0, 255]).count();
        assert_eq!(black, 100 * 100);
    }

    #[test]
    fn last_sheet_may_be_partial() {
        let s = PrintSettings { nup: 4, ..Default::default() };
        let pages: Vec<_> = (0..5).map(|_| page(10, 10, [255, 0, 0, 255], 10.0, 10.0)).collect();
        assert_eq!(sheet_count(pages.len(), s.nup), 2);
        let second = compose_sheet(&pages, &s, 1, 36.0);
        let (w, h) = second.dimensions();
        assert_eq!(second.get_pixel(w / 4, h / 4).0, [255, 0, 0, 255], "slot 1 ocupado");
        assert_eq!(second.get_pixel(3 * w / 4, 3 * h / 4).0, [255, 255, 255, 255], "slot 4 vazio");
    }

    #[test]
    fn eight_and_twelve_up_grids() {
        assert_eq!(nup_grid(8, false), (2, 4));
        assert_eq!(nup_grid(8, true), (4, 2));
        assert_eq!(nup_grid(12, false), (3, 4));
        assert_eq!(nup_grid(7, false), (1, 1), "valores fora da lista caem em 1");
    }

    #[test]
    fn repeat_fills_sheet_with_each_page() {
        let s = PrintSettings { nup: 4, repeat: true, page_range: "2-3".into(), ..Default::default() };
        assert_eq!(s.print_order(5).unwrap(), vec![1, 1, 1, 1, 2, 2, 2, 2]);
        let s = PrintSettings { nup: 1, repeat: true, ..Default::default() };
        assert_eq!(s.print_order(2).unwrap(), vec![0, 1], "repeat sem N-up não duplica");
    }

    #[test]
    fn identity_transform_is_not_stored() {
        let mut s = PrintSettings::default();
        s.set_transform(3, ItemTransform::default().with_zoom(2.0));
        assert_eq!(s.transforms.len(), 1);
        s.set_transform(3, ItemTransform::default());
        assert!(s.transforms.is_empty());
        assert_eq!(ItemTransform::default().with_zoom(99.0).zoom, ItemTransform::ZOOM_MAX);
        assert_eq!(ItemTransform::default().moved(5.0, -5.0).offset_x, 1.0);
    }

    #[test]
    fn geometry_hit_testing() {
        let s = PrintSettings { nup: 4, ..Default::default() };
        let g = SheetGeometry::new(&s, 72.0);
        assert_eq!(g.slot_at(0.25, 0.25), Some(0));
        assert_eq!(g.slot_at(0.75, 0.25), Some(1));
        assert_eq!(g.slot_at(0.25, 0.75), Some(2));
        assert_eq!(g.slot_at(0.75, 0.75), Some(3));
        assert_eq!(g.slot_at(0.001, 0.001), None, "margem não é célula");
        let (dx, _) = g.sheet_to_cell_delta(0.1, 0.0);
        assert!(dx > 0.2 && dx < 0.25, "célula ≈ metade da folha: {dx}");
    }

    /// Duas cores: metade esquerda vermelha, direita azul.
    fn split_page() -> SourcePage {
        let mut img = RgbaImage::from_pixel(100, 100, image::Rgba([0, 0, 255, 255]));
        for y in 0..100 {
            for x in 0..50 {
                img.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
            }
        }
        SourcePage { index: 0, image: img, width_pt: 100.0, height_pt: 100.0 }
    }

    #[test]
    fn crop_removes_part_of_image() {
        // Vermelho em x < 60, azul no resto; cortar 40 % da direita deixa só vermelho.
        let mut img = RgbaImage::from_pixel(100, 100, image::Rgba([0, 0, 255, 255]));
        for y in 0..100 {
            for x in 0..60 {
                img.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
            }
        }
        let page = SourcePage { index: 0, image: img, width_pt: 100.0, height_pt: 100.0 };
        let mut s = PrintSettings::default();
        let crop = Crop { right: 0.4, ..Default::default() };
        s.set_transform(0, ItemTransform { crop, ..Default::default() });
        let sheet = compose_sheet(&[page], &s, 0, 36.0);
        assert_eq!(sheet.pixels().filter(|p| p.0 == [0, 0, 255, 255]).count(), 0, "parte azul foi cortada");
        assert!(sheet.pixels().filter(|p| p.0 == [255, 0, 0, 255]).count() > 10_000);
    }

    #[test]
    fn crop_is_limited_per_edge() {
        let c = Crop { left: 0.9, top: -1.0, right: 0.2, bottom: 0.45 }.clamped();
        assert_eq!((c.left, c.top, c.right, c.bottom), (Crop::MAX, 0.0, 0.2, 0.45));
    }

    #[test]
    fn zoom_and_offset_move_content() {
        let g = SheetGeometry::new(&PrintSettings::default(), 36.0);
        let center = (g.width / 2, g.height / 2);

        // Zoom 50 %: bordas da célula ficam brancas.
        let mut s = PrintSettings::default();
        s.set_transform(0, ItemTransform::default().with_zoom(0.5));
        let sheet = compose_sheet(&[split_page()], &s, 0, 36.0);
        assert_eq!(sheet.get_pixel(g.margin + 2, center.1).0, [255, 255, 255, 255]);

        // Deslocar 30 % da célula para a direita: o centro passa a ser vermelho.
        let mut s = PrintSettings::default();
        s.set_transform(0, ItemTransform::default().moved(0.3, 0.0));
        let sheet = compose_sheet(&[split_page()], &s, 0, 36.0);
        assert_eq!(sheet.get_pixel(center.0 + 2, center.1).0, [255, 0, 0, 255]);

        // Zoom 4×: não estoura e o centro continua na fronteira vermelho/azul
        // (a interpolação suaviza ~1 px da fonte = ~20 px aqui; medimos a 40 px).
        let mut s = PrintSettings::default();
        s.set_transform(0, ItemTransform::default().with_zoom(4.0));
        let sheet = compose_sheet(&[split_page()], &s, 0, 36.0);
        assert_eq!(sheet.dimensions(), (g.width, g.height));
        assert_eq!(sheet.get_pixel(center.0 - 40, center.1).0, [255, 0, 0, 255]);
        assert_eq!(sheet.get_pixel(center.0 + 40, center.1).0, [0, 0, 255, 255]);
    }

    #[test]
    fn transform_applies_only_to_its_page() {
        let mut s = PrintSettings { nup: 2, ..Default::default() };
        s.set_transform(1, ItemTransform::default().with_zoom(0.2));
        let mut a = split_page();
        let mut b = split_page();
        a.index = 0;
        b.index = 1;
        let sheet = compose_sheet(&[a, b], &s, 0, 36.0);
        let g = SheetGeometry::new(&s, 36.0);
        let (_, y1) = g.cell_origin(1);
        // Página 0 cheia; página 1 reduzida → canto da célula 1 branco.
        assert_ne!(sheet.get_pixel(g.width / 4, g.margin + g.cell_h / 2).0, [255, 255, 255, 255]);
        assert_eq!(sheet.get_pixel(g.margin + 3, y1 + 3).0, [255, 255, 255, 255]);
    }

    #[test]
    fn outline_marks_cell_border() {
        let s = PrintSettings { nup: 2, ..Default::default() };
        let g = SheetGeometry::new(&s, 36.0);
        let mut img = RgbaImage::from_pixel(g.width, g.height, image::Rgba([255, 255, 255, 255]));
        outline_slot(&mut img, &g, 1, [0, 120, 255, 255]);
        let (x, y) = g.cell_origin(1);
        assert_eq!(img.get_pixel(x + 5, y).0, [0, 120, 255, 255]);
        assert_eq!(img.get_pixel(g.width / 2, g.height / 2 - 20).0, [255, 255, 255, 255]);
    }
}
