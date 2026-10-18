//! Diálogos modais: foto aleatória (opções do picsum), atalhos e sobre.

use cosmic::Element;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, settings};
use printerf::layout::PrintSettings;
use printerf::samples::{MAX_PHOTOS, PICSUM_MAX_SIDE, PhotoShape, PhotoSize, PicsumOptions};

use super::{Message, OpenTarget, menu::SHORTCUTS};

pub const ICON_SVG: &[u8] = include_bytes!("../../res/icons/hicolor/scalable/apps/io.github.kelthon.PrinterF.svg");

pub enum Dialog {
    Photo(PhotoForm),
    Shortcuts,
    About,
}

const SIZE_NAMES: [&str; 5] = ["Pequena", "Média", "Grande", "Página inteira", "Personalizada"];
const SHAPE_LABELS: &[&str] = &["Paisagem (3:2)", "Retrato (2:3)", "Quadrada (1:1)"];

/// Formulário do diálogo “Foto aleatória”. Tudo é opcional: o padrão é 1 foto média.
pub struct PhotoForm {
    pub target: OpenTarget,
    pub opts: PicsumOptions,
    width: String,
    height: String,
    id: String,
}

#[derive(Debug, Clone)]
pub enum PhotoMsg {
    Size(usize),
    Shape(usize),
    Width(String),
    Height(String),
    Count(i32),
    Grayscale(bool),
    Blur(u8),
    Id(String),
}

impl PhotoForm {
    pub fn new(target: OpenTarget) -> Self {
        Self {
            target,
            opts: PicsumOptions { count: 1, ..Default::default() },
            width: "1200".into(),
            height: "800".into(),
            id: String::new(),
        }
    }

    pub fn update(&mut self, msg: PhotoMsg) {
        match msg {
            PhotoMsg::Size(i) => self.opts.size = PhotoSize::PRESETS[i],
            PhotoMsg::Shape(i) => self.opts.shape = PhotoShape::ALL[i],
            PhotoMsg::Width(s) => self.width = s.chars().filter(char::is_ascii_digit).take(4).collect(),
            PhotoMsg::Height(s) => self.height = s.chars().filter(char::is_ascii_digit).take(4).collect(),
            PhotoMsg::Count(d) => self.opts.count = (self.opts.count as i32 + d).clamp(1, MAX_PHOTOS as i32) as u8,
            PhotoMsg::Grayscale(on) => self.opts.grayscale = on,
            PhotoMsg::Blur(b) => self.opts.blur = b,
            PhotoMsg::Id(s) => self.id = s.chars().filter(char::is_ascii_digit).take(5).collect(),
        }
        if let PhotoSize::Custom { .. } = self.opts.size {
            self.opts.size =
                PhotoSize::Custom { width: self.width.parse().unwrap_or(0), height: self.height.parse().unwrap_or(0) };
        }
    }

    /// Opções finais, validadas.
    pub fn options(&self) -> Result<PicsumOptions, String> {
        let mut o = self.opts.clone();
        if let PhotoSize::Custom { width, height } = o.size {
            let ok = |v: u32| (16..=PICSUM_MAX_SIDE).contains(&v);
            if !ok(width) || !ok(height) {
                return Err(format!("Tamanho personalizado deve ficar entre 16 e {PICSUM_MAX_SIDE} px por lado"));
            }
        }
        o.id = if self.id.is_empty() { None } else { self.id.parse().ok() };
        if o.id.is_some() {
            o.count = 1; // mesma foto N vezes não faz sentido; use “Repetir” no editor
        }
        Ok(o)
    }

    fn size_index(&self) -> usize {
        match self.opts.size {
            PhotoSize::Small => 0,
            PhotoSize::Medium => 1,
            PhotoSize::Large => 2,
            PhotoSize::FullPage => 3,
            PhotoSize::Custom { .. } => 4,
        }
    }

    fn view<'a>(&'a self, sheet: &PrintSettings) -> Element<'a, Message> {
        // Rótulos com medida impressa, ex. “Média · 15,2 × 10,2 cm”.
        let labels: Vec<String> = PhotoSize::PRESETS
            .iter()
            .zip(SIZE_NAMES)
            .map(|(size, name)| {
                let o = PicsumOptions { size: *size, ..self.opts.clone() };
                match size {
                    PhotoSize::Custom { .. } => name.to_string(),
                    _ => {
                        let (w, h) = o.print_size_cm(sheet);
                        format!("{name} · {w:.1} × {h:.1} cm").replace('.', ",")
                    }
                }
            })
            .collect();
        let is_custom = matches!(self.opts.size, PhotoSize::Custom { .. });
        let shape_applies = !is_custom && self.opts.size != PhotoSize::FullPage;

        let mut form = settings::section()
            .add(settings::item(
                "Tamanho",
                widget::dropdown(labels, Some(self.size_index()), |i| Message::Photo(PhotoMsg::Size(i))),
            ))
            .add(settings::item(
                "Formato",
                widget::dropdown(
                    SHAPE_LABELS,
                    Some(PhotoShape::ALL.iter().position(|s| *s == self.opts.shape).unwrap_or(0)),
                    |i| Message::Photo(PhotoMsg::Shape(i)),
                ),
            ));
        if !shape_applies && !is_custom {
            form = form.add(widget::text::caption("Página inteira usa a proporção da folha."));
        }
        if is_custom {
            let (w, h) = PicsumOptions { size: self.opts.size, ..Default::default() }.print_size_cm(sheet);
            form = form
                .add(settings::item(
                    "Largura × altura (px)",
                    widget::Row::new()
                        .spacing(6)
                        .align_y(Alignment::Center)
                        .push(
                            widget::text_input("largura", &self.width)
                                .on_input(|s| Message::Photo(PhotoMsg::Width(s)))
                                .width(Length::Fixed(80.0)),
                        )
                        .push(widget::text::body("×"))
                        .push(
                            widget::text_input("altura", &self.height)
                                .on_input(|s| Message::Photo(PhotoMsg::Height(s)))
                                .width(Length::Fixed(80.0)),
                        ),
                ))
                .add(widget::text::caption(
                    format!("≈ {w:.1} × {h:.1} cm impressos a 150 DPI · máximo {PICSUM_MAX_SIDE} px").replace('.', ","),
                ));
        }

        let count = widget::Row::new()
            .spacing(4)
            .align_y(Alignment::Center)
            .push(
                widget::button::icon(widget::icon::from_name("list-remove-symbolic"))
                    .on_press(Message::Photo(PhotoMsg::Count(-1))),
            )
            .push(
                widget::text::body(self.opts.count().to_string()).width(Length::Fixed(28.0)).align_x(Alignment::Center),
            )
            .push(
                widget::button::icon(widget::icon::from_name("list-add-symbolic"))
                    .on_press(Message::Photo(PhotoMsg::Count(1))),
            );

        let extras = settings::section()
            .title("Opcional")
            .add(settings::item("Quantidade de fotos", count))
            .add(settings::item(
                "Preto e branco",
                widget::toggler(self.opts.grayscale).on_toggle(|b| Message::Photo(PhotoMsg::Grayscale(b))),
            ))
            .add(settings::item(
                format!("Desfoque {}", if self.opts.blur == 0 { "—".into() } else { self.opts.blur.to_string() }),
                widget::slider(0..=10u8, self.opts.blur, |b| Message::Photo(PhotoMsg::Blur(b)))
                    .width(Length::Fixed(160.0)),
            ))
            .add(settings::item(
                "Foto específica (ID)",
                widget::text_input("aleatória", &self.id)
                    .on_input(|s| Message::Photo(PhotoMsg::Id(s)))
                    .width(Length::Fixed(110.0)),
            ));

        let hint = match self.target {
            OpenTarget::Append(_) => "As fotos serão adicionadas à aba atual.",
            OpenTarget::NewTab => "As fotos abrem numa nova aba; várias fotos já vêm em N-up.",
        };

        widget::dialog()
            .title("Foto aleatória")
            .icon(widget::icon::from_name("image-x-generic-symbolic").size(48))
            .body("Fotos de picsum.photos para testar cor e qualidade de impressão.")
            .control(widget::Column::new().spacing(16).push(form).push(extras).push(widget::text::caption(hint)))
            .primary_action(widget::button::suggested("Baixar").on_press(Message::PhotoDownload))
            .secondary_action(widget::button::standard("Cancelar").on_press(Message::CloseDialog))
            .into()
    }
}

impl Dialog {
    pub fn view<'a>(&'a self, sheet: Option<&PrintSettings>) -> Element<'a, Message> {
        match self {
            Dialog::Photo(form) => form.view(sheet.unwrap_or(&PrintSettings::default())),
            Dialog::Shortcuts => {
                let mut list = settings::section();
                for (key, what) in SHORTCUTS {
                    list = list.add(settings::item(*what, widget::text::heading(*key)));
                }
                widget::dialog()
                    .title("Atalhos de teclado")
                    .control(widget::scrollable(list).height(Length::Fixed(420.0)))
                    .primary_action(widget::button::suggested("Fechar").on_press(Message::CloseDialog))
                    .into()
            }
            Dialog::About => widget::dialog()
                .title(format!("PrinterF {}", env!("CARGO_PKG_VERSION")))
                .icon(widget::svg(widget::svg::Handle::from_memory(ICON_SVG)).width(64).height(64))
                .body(
                    "Como o printf do C — mas para o papel.\n\n\
                     Prepara, organiza e envia arquivos para impressão; a impressora e o driver \
                     ficam com o sistema (CUPS + diálogo de impressão do XDG Portal).\n\n\
                     Ícone derivado do Pop icon theme (System76, CC-BY-SA-4.0). \
                     Fotos de exemplo: Lorem Picsum (picsum.photos).",
                )
                .primary_action(widget::button::suggested("Fechar").on_press(Message::CloseDialog))
                .secondary_action(
                    widget::button::standard("Guia de uso").on_press(Message::Menu(super::menu::Action::Guide)),
                )
                .into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn photo_form_defaults_and_validation() {
        let mut f = PhotoForm::new(OpenTarget::NewTab);
        assert_eq!(f.options().unwrap(), PicsumOptions { count: 1, ..Default::default() });

        f.update(PhotoMsg::Size(4)); // personalizada
        f.update(PhotoMsg::Width("12a00".into()));
        f.update(PhotoMsg::Height("900".into()));
        assert_eq!(f.options().unwrap().size, PhotoSize::Custom { width: 1200, height: 900 });
        f.update(PhotoMsg::Width("9".into()));
        assert!(f.options().is_err(), "lado < 16 px é recusado");

        f.update(PhotoMsg::Size(1));
        f.update(PhotoMsg::Count(50));
        assert_eq!(f.options().unwrap().count, MAX_PHOTOS);
        f.update(PhotoMsg::Id("237".into()));
        let o = f.options().unwrap();
        assert_eq!((o.id, o.count), (Some(237), 1), "foto específica baixa uma só");
    }
}
