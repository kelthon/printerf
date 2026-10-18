//! Estado de uma aba: um documento, suas configurações e o preview.

use std::sync::Arc;

use cosmic::widget::image::Handle;
use image::RgbaImage;
use printerf::document::{Document, Kind};
use printerf::layout::{ItemTransform, PrintSettings, SheetGeometry, SourcePage, outline_slot};

/// 72 DPI ≈ 595×842 px por A4: nítido o bastante e barato de recompor a cada arrasto.
pub const PREVIEW_DPI: f32 = 72.0;
/// Cor do realce da imagem selecionada no preview.
const SELECTION_RGBA: [u8; 4] = [64, 132, 255, 255];

pub struct Tab {
    pub doc: Document,
    pub settings: PrintSettings,
    pub range_error: Option<String>,
    /// Páginas rasterizadas a [`PREVIEW_DPI`], `cache[i].index == i`.
    pub cache: Option<Arc<Vec<SourcePage>>>,
    /// Folhas compostas (sem realce).
    pub sheets: Vec<Arc<RgbaImage>>,
    pub sheet: usize,
    /// Página-fonte selecionada para ajuste livre.
    pub selected: Option<usize>,
    /// Imagem exibida (folha atual + realce), recriada só quando algo muda.
    pub display: Option<Handle>,
    /// Descarta composições obsoletas.
    pub generation: u64,
    pub busy: bool,
}

impl Tab {
    pub fn new(doc: Document, settings: PrintSettings) -> Self {
        Self {
            doc,
            settings,
            range_error: None,
            cache: None,
            sheets: vec![],
            sheet: 0,
            selected: None,
            display: None,
            generation: 0,
            busy: true,
        }
    }

    pub fn is_images(&self) -> bool {
        self.doc.kind == Kind::Image
    }

    pub fn validate_range(&mut self) {
        self.range_error = self.settings.print_order(self.doc.page_count()).err();
    }

    /// Página-fonte em cada célula da folha atual.
    pub fn slots_of_current_sheet(&self) -> Vec<usize> {
        let per = self.settings.nup.max(1) as usize;
        self.settings
            .print_order(self.doc.page_count())
            .map(|o| o.into_iter().skip(self.sheet * per).take(per).collect())
            .unwrap_or_default()
    }

    /// Página sob o ponto `(fx, fy)` (frações da folha) na folha atual.
    pub fn page_at(&self, fx: f32, fy: f32) -> Option<usize> {
        let slot = SheetGeometry::new(&self.settings, PREVIEW_DPI).slot_at(fx, fy)?;
        self.slots_of_current_sheet().get(slot).copied()
    }

    pub fn selected_transform(&self) -> Option<ItemTransform> {
        self.selected.map(|p| self.settings.transform(p))
    }

    /// Recria a imagem exibida: folha atual + contorno nas células da página selecionada.
    pub fn refresh_display(&mut self) {
        self.sheet = self.sheet.min(self.sheets.len().saturating_sub(1));
        let Some(sheet) = self.sheets.get(self.sheet) else {
            self.display = None;
            return;
        };
        let mut img = (**sheet).clone();
        if let Some(sel) = self.selected {
            let g = SheetGeometry::new(&self.settings, PREVIEW_DPI);
            for (slot, page) in self.slots_of_current_sheet().into_iter().enumerate() {
                if page == sel {
                    outline_slot(&mut img, &g, slot, SELECTION_RGBA);
                }
            }
        }
        self.display = Some(Handle::from_rgba(img.width(), img.height(), img.into_raw()));
    }

    pub fn title(&self) -> String {
        let t = self.doc.title();
        if t.chars().count() > 28 { format!("{}…", t.chars().take(27).collect::<String>()) } else { t }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(pages: usize, settings: PrintSettings) -> Tab {
        let mut doc = Document::image("a.png".into(), (100.0, 100.0));
        for i in 1..pages {
            doc.append_images(Document::image(format!("{i}.png").into(), (100.0, 100.0))).unwrap();
        }
        Tab::new(doc, settings)
    }

    #[test]
    fn hit_testing_maps_cells_to_pages() {
        let mut t = tab(6, PrintSettings { nup: 4, ..Default::default() });
        assert_eq!(t.slots_of_current_sheet(), vec![0, 1, 2, 3]);
        assert_eq!(t.page_at(0.75, 0.75), Some(3));
        t.sheet = 1;
        assert_eq!(t.slots_of_current_sheet(), vec![4, 5]);
        assert_eq!(t.page_at(0.25, 0.25), Some(4));
        assert_eq!(t.page_at(0.25, 0.75), None, "célula vazia na última folha");
    }

    #[test]
    fn repeat_mode_hits_same_page_everywhere() {
        let t = tab(2, PrintSettings { nup: 4, repeat: true, ..Default::default() });
        assert_eq!(t.slots_of_current_sheet(), vec![0, 0, 0, 0]);
        assert_eq!(t.page_at(0.75, 0.75), Some(0));
    }

    #[test]
    fn display_highlights_selection_and_clamps_sheet() {
        let mut t = tab(1, PrintSettings::default());
        t.sheets = vec![Arc::new(RgbaImage::from_pixel(10, 10, image::Rgba([255, 255, 255, 255])))];
        t.sheet = 5;
        t.selected = Some(0);
        t.refresh_display();
        assert_eq!(t.sheet, 0);
        assert!(t.display.is_some());
        t.sheets.clear();
        t.refresh_display();
        assert!(t.display.is_none());
    }

    #[test]
    fn long_titles_are_shortened() {
        let t = Tab::new(
            Document::image("um-nome-de-arquivo-muito-longo-mesmo.png".into(), (1.0, 1.0)),
            Default::default(),
        );
        assert_eq!(t.title().chars().count(), 28);
        assert!(t.title().ends_with('…'));
    }
}
