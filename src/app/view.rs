//! Views: tela inicial, painel lateral (configurações + rodapé de impressão),
//! abas + preview interativo, painel de status e fila.

use cosmic::Element;
use cosmic::iced::{Alignment, ContentFit, Length, mouse};
use cosmic::widget::{self, settings};
use printerf::document::Kind;
use printerf::layout::{FitMode, NUP_CHOICES, Orientation, PaperSize, Rotation};

use super::dialogs::ICON_SVG;
use super::tab::Tab;
use super::{App, CROP_MAX, CropEdge, Message, OpenTarget, ScrollDelta, menu::Action};

const ORIENTATION_LABELS: &[&str] = &["Retrato", "Paisagem"];
const ROTATION_LABELS: &[&str] = &["0°", "90°", "180°", "270°"];
const FIT_LABELS: &[&str] = &["Ajustar à célula", "Preencher a célula", "Tamanho original"];
const PAPER_LABELS: &[&str] = &["A4 (210 × 297 mm)", "Carta (8,5 × 11 in)"];
const NUP_LABELS: &[&str] = &[
    "1 por folha",
    "2 por folha",
    "4 por folha",
    "6 por folha",
    "8 por folha",
    "9 por folha",
    "12 por folha",
    "16 por folha",
];
const ASIDE_WIDTH: f32 = 340.0;
const STATUS_WIDTH: f32 = 290.0;

pub fn main(app: &App) -> Element<'_, Message> {
    let mut root = widget::Column::new().spacing(8).padding([0, 8, 8, 8]);
    if let Some(n) = &app.notice {
        root = root.push(widget::warning(n.clone()).on_close(Message::DismissNotice));
    }

    let mut body = widget::Row::new().spacing(12).height(Length::Fill);
    match app.active() {
        Some(tab) => {
            if app.show_aside {
                body = body.push(aside(app, tab));
            }
            body = body.push(center(app, tab));
        }
        None => body = body.push(welcome(app)),
    }
    if app.show_status {
        body = body.push(status_panel(app));
    }
    root.push(body).into()
}

// ---------------------------------------------------------------------------
// Tela inicial
// ---------------------------------------------------------------------------

fn welcome(app: &App) -> Element<'_, Message> {
    let card = |icon: &'static str, title: &'static str, hint: &'static str, msg: Message| {
        widget::button::custom(
            widget::Column::new()
                .spacing(8)
                .align_x(Alignment::Center)
                .push(widget::icon::from_name(icon).size(40))
                .push(widget::text::heading(title))
                .push(widget::text::caption(hint))
                .width(Length::Fixed(190.0))
                .padding(16),
        )
        .class(cosmic::theme::Button::Standard)
        .on_press(msg)
    };
    let content = widget::Column::new()
        .spacing(16)
        .align_x(Alignment::Center)
        .push(widget::svg(widget::svg::Handle::from_memory(ICON_SVG)).width(112).height(112))
        .push(widget::text::title3("PrinterF"))
        .push(widget::text::body("Abra um PDF, imagens ou texto para organizar e imprimir."))
        .push(
            widget::Row::new()
                .spacing(12)
                .push(card(
                    "document-open-symbolic",
                    "Abrir arquivos",
                    "Ctrl+O · várias imagens = 1 aba",
                    Message::Menu(Action::Open),
                ))
                .push(card(
                    "image-x-generic-symbolic",
                    "Foto aleatória",
                    "Ctrl+R · picsum.photos",
                    Message::OpenPhotoDialog(OpenTarget::NewTab),
                ))
                .push(card(
                    "document-print-preview-symbolic",
                    "Página de teste",
                    "Ctrl+T · cores e alinhamento",
                    Message::Menu(Action::TestPage),
                )),
        )
        .push(widget::text::caption("A impressora e o driver são escolhidos pelo sistema na hora de imprimir."));
    let content: Element<'_, Message> = if let Some(l) = app.loading {
        widget::Column::new()
            .spacing(12)
            .align_x(Alignment::Center)
            .push(widget::progress_bar::indeterminate_circular())
            .push(widget::text::body(l))
            .into()
    } else {
        content.into()
    };
    widget::container(content).center(Length::Fill).into()
}

// ---------------------------------------------------------------------------
// Painel lateral
// ---------------------------------------------------------------------------

fn aside<'a>(app: &'a App, tab: &'a Tab) -> Element<'a, Message> {
    let s = &tab.settings;
    let pos = |v: usize| Some(v);

    let tray_idx = s.tray.as_ref().and_then(|t| app.tray_labels.iter().position(|l| l == t)).unwrap_or(0);
    let duplex_supported = app.status.as_ref().is_none_or(|st| st.supports_duplex());

    let paper = settings::section()
        .title("Papel")
        .add(settings::item(
            "Tamanho",
            widget::dropdown(
                PAPER_LABELS,
                pos(PaperSize::ALL.iter().position(|p| *p == s.paper).unwrap_or(0)),
                Message::Paper,
            ),
        ))
        .add(settings::item(
            "Orientação",
            widget::dropdown(
                ORIENTATION_LABELS,
                pos((s.orientation == Orientation::Landscape) as usize),
                Message::Orientation,
            ),
        ));

    let mut layout = settings::section()
        .title("Layout")
        .add(settings::item(
            "Por folha",
            widget::dropdown(NUP_LABELS, pos(NUP_CHOICES.iter().position(|n| *n == s.nup).unwrap_or(0)), Message::NUp),
        ))
        .add(settings::item(
            "Escala",
            widget::dropdown(FIT_LABELS, pos(FitMode::ALL.iter().position(|f| *f == s.fit).unwrap_or(0)), Message::Fit),
        ))
        .add(settings::item(
            "Rotação",
            widget::dropdown(
                ROTATION_LABELS,
                pos(Rotation::ALL.iter().position(|r| *r == s.rotation).unwrap_or(0)),
                Message::Rotation,
            ),
        ));
    if s.nup > 1 {
        let what = if tab.is_images() { "Repetir cada imagem na folha" } else { "Repetir cada página na folha" };
        layout = layout.add(settings::item(what, widget::toggler(s.repeat).on_toggle(Message::Repeat)));
    }

    let mut range_col = widget::Column::new()
        .spacing(4)
        .push(widget::text_input("Todas (ex.: 1-3, 5, 8)", &s.page_range).on_input(Message::PageRange));
    if let Some(e) = &tab.range_error {
        range_col = range_col.push(widget::text::caption(e.clone()));
    }
    let copies = widget::Row::new()
        .spacing(4)
        .align_y(Alignment::Center)
        .push(widget::button::icon(widget::icon::from_name("list-remove-symbolic")).on_press(Message::Copies(-1)))
        .push(widget::text::body(s.copies.to_string()).width(Length::Fixed(32.0)).align_x(Alignment::Center))
        .push(widget::button::icon(widget::icon::from_name("list-add-symbolic")).on_press(Message::Copies(1)));
    let mut job = settings::section()
        .title("Envio direto")
        .add(settings::item(if tab.is_images() { "Imagens" } else { "Páginas" }, range_col))
        .add(settings::item("Cópias", copies))
        .add(settings::item("Bandeja", widget::dropdown(&app.tray_labels, Some(tray_idx), Message::Tray)))
        .add(settings::item(
            if duplex_supported { "Frente e verso" } else { "Frente e verso (não suportado)" },
            widget::toggler(s.duplex).on_toggle_maybe(duplex_supported.then_some(Message::Duplex)),
        ));
    if tab.doc.kind == Kind::Pdf {
        job = job.add(settings::item(
            "PDF vetorial (CUPS compõe)",
            widget::toggler(s.vector_passthrough).on_toggle(Message::Passthrough),
        ));
    }
    job = job.add(widget::text::caption("No diálogo do sistema, cópias e frente e verso são escolhidos lá."));

    let scroll = widget::scrollable(
        widget::Column::new()
            .spacing(16)
            .padding([0, 12, 0, 0])
            .push(item_editor(tab))
            .push(layout)
            .push(paper)
            .push(job),
    )
    .height(Length::Fill);

    widget::Column::new().spacing(8).width(Length::Fixed(ASIDE_WIDTH)).push(scroll).push(print_footer(app, tab)).into()
}

/// Ajuste livre da imagem selecionada (zoom, posição, corte).
fn item_editor<'a>(tab: &'a Tab) -> Element<'a, Message> {
    let Some((page, t)) = tab.selected.zip(tab.selected_transform()) else {
        return settings::section()
            .title("Ajuste livre")
            .add(widget::text::caption(
                "Clique numa imagem do preview para selecioná-la. Depois arraste para mover, \
                 use a roda do mouse para o tamanho e os controles abaixo para cortar.",
            ))
            .into();
    };
    let pct = |v: f32| format!("{:.0}%", v * 100.0);
    let crop_slider = |label: &'static str, edge: CropEdge, v: f32| {
        settings::item(
            format!("{label} {}", pct(v)),
            widget::slider(0.0..=CROP_MAX, v, move |x| Message::Crop(edge, x)).step(0.01).width(Length::Fixed(150.0)),
        )
    };
    settings::section()
        .title(format!("Ajuste livre · {}", tab.doc.page_label(page)))
        .add(settings::item(
            format!("Tamanho {}", pct(t.zoom)),
            widget::slider(0.1..=4.0, t.zoom, Message::Zoom).step(0.01).width(Length::Fixed(150.0)),
        ))
        .add(crop_slider("Corte esquerda", CropEdge::Left, t.crop.left))
        .add(crop_slider("Corte direita", CropEdge::Right, t.crop.right))
        .add(crop_slider("Corte topo", CropEdge::Top, t.crop.top))
        .add(crop_slider("Corte base", CropEdge::Bottom, t.crop.bottom))
        .add(
            widget::Row::new()
                .spacing(8)
                .push(widget::button::standard("Centralizar").on_press(Message::CenterItem))
                .push(widget::button::standard("Redefinir").on_press(Message::ResetItem))
                .push(widget::space::horizontal())
                .push(widget::button::text("Concluir").on_press(Message::Deselect)),
        )
        .into()
}

/// Rodapé fixo: ação principal + envio direto.
fn print_footer<'a>(app: &'a App, tab: &'a Tab) -> Element<'a, Message> {
    let ready = !app.busy_printing && tab.range_error.is_none() && !tab.sheets.is_empty();
    let sheets = tab.sheets.len();
    let summary = format!(
        "{sheets} folha(s) · {} · {}",
        tab.settings.paper.cups_media(),
        if tab.settings.orientation == Orientation::Landscape { "paisagem" } else { "retrato" }
    );
    let print = widget::button::suggested(if app.busy_printing { "Imprimindo…" } else { "Imprimir…" })
        .leading_icon(widget::icon::from_name("document-print-symbolic"))
        .width(Length::Fill)
        .on_press_maybe(ready.then_some(Message::PrintNative));
    let direct_label = match app.selected_printer.and_then(|i| app.printers.get(i)) {
        Some(p) => format!("Enviar direto para {}", p.display_name()),
        None => "Nenhuma impressora para envio direto".into(),
    };
    let direct = widget::button::text(direct_label)
        .width(Length::Fill)
        .on_press_maybe((ready && app.selected_printer.is_some()).then_some(Message::PrintDirect));
    widget::container(
        widget::Column::new().spacing(6).padding(12).push(widget::text::caption(summary)).push(print).push(direct),
    )
    .class(cosmic::theme::Container::Card)
    .into()
}

// ---------------------------------------------------------------------------
// Abas + preview
// ---------------------------------------------------------------------------

fn center<'a>(app: &'a App, tab: &'a Tab) -> Element<'a, Message> {
    let tabs = widget::tab_bar::horizontal(&app.tabs).on_activate(Message::TabActivate).on_close(Message::TabClose);

    let preview: Element<'a, Message> = match (&tab.display, app.loading) {
        (_, Some(l)) => busy(l),
        (None, None) => busy(if tab.busy { "Gerando preview…" } else { "Nada a imprimir neste intervalo" }),
        (Some(handle), None) => {
            let (sw, sh) = tab.settings.sheet_pt();
            let aspect = sw / sh;
            let handle = handle.clone();
            let dragging = app.drag.is_some();
            widget::responsive(move |size| {
                // Maior retângulo com a proporção da folha que cabe na área.
                let (avail_w, avail_h) = ((size.width - 16.0).max(50.0), (size.height - 16.0).max(50.0));
                let (w, h) =
                    if avail_w / avail_h > aspect { (avail_h * aspect, avail_h) } else { (avail_w, avail_w / aspect) };
                let img = widget::image(handle.clone())
                    .content_fit(ContentFit::Fill)
                    .width(Length::Fixed(w))
                    .height(Length::Fixed(h));
                let area = widget::mouse_area(widget::container(img).class(cosmic::theme::Container::Card))
                    .on_move(move |p| Message::PreviewMove(p.x / w, p.y / h))
                    .on_press(Message::PreviewPress)
                    .on_release(Message::PreviewRelease)
                    .on_exit(Message::PreviewRelease)
                    .on_scroll(|d| {
                        Message::PreviewScroll(match d {
                            ScrollDelta::Lines { y, .. } => y,
                            ScrollDelta::Pixels { y, .. } => y / 40.0,
                        })
                    })
                    .interaction(if dragging { mouse::Interaction::Grabbing } else { mouse::Interaction::Pointer });
                widget::container(area).center(Length::Fill).into()
            })
            .into()
        }
    };

    let total = tab.sheets.len();
    let nav = widget::Row::new()
        .spacing(8)
        .align_y(Alignment::Center)
        .push(
            widget::button::icon(widget::icon::from_name("go-previous-symbolic"))
                .on_press_maybe((tab.sheet > 0).then_some(Message::PrevSheet)),
        )
        .push(widget::text::body(if total == 0 {
            "—".to_string()
        } else {
            format!("Folha {} de {}{}", tab.sheet + 1, total, if tab.busy { " · atualizando…" } else { "" })
        }))
        .push(
            widget::button::icon(widget::icon::from_name("go-next-symbolic"))
                .on_press_maybe((tab.sheet + 1 < total).then_some(Message::NextSheet)),
        )
        .push(widget::space::horizontal())
        .push(widget::text::caption(if tab.selected.is_some() {
            "Arraste para mover · roda do mouse para tamanho · Esc conclui"
        } else {
            "Clique numa imagem para ajustá-la"
        }));

    widget::Column::new().spacing(8).width(Length::Fill).push(tabs).push(preview).push(nav).into()
}

fn busy<'a>(label: &'a str) -> Element<'a, Message> {
    widget::container(
        widget::Column::new()
            .spacing(12)
            .align_x(Alignment::Center)
            .push(widget::progress_bar::indeterminate_circular())
            .push(widget::text::body(label)),
    )
    .center(Length::Fill)
    .into()
}

// ---------------------------------------------------------------------------
// Status e fila
// ---------------------------------------------------------------------------

fn status_panel(app: &App) -> Element<'_, Message> {
    let mut col = widget::Column::new().spacing(12);

    let printer = settings::section().title("Impressora").add(widget::dropdown(
        &app.printer_labels,
        app.selected_printer,
        Message::SelectPrinter,
    ));
    col = col.push(printer);

    let mut status = settings::section().title("Status");
    match &app.status {
        Some(st) => {
            status = status.add(settings::item("Estado", widget::text::heading(st.summary())));
            if let Some(msg) = &st.message {
                status = status.add(widget::text::caption(msg.clone()));
            }
            for c in st.conditions.iter().skip(1) {
                status = status.add(widget::text::caption(c.label()));
            }
            for m in &st.markers {
                let label = match m.level {
                    Some(l) => format!("{} — {l}%", m.name),
                    None => format!("{} — ?", m.name),
                };
                status = status.add(
                    widget::Column::new()
                        .spacing(4)
                        .push(widget::text::caption(label))
                        .push(widget::progress_bar::determinate_linear(m.level.unwrap_or(0) as f32 / 100.0)),
                );
            }
        }
        None => {
            status = status.add(widget::text::body(match (&app.status_error, app.printers.is_empty()) {
                (_, true) => "Nenhuma impressora no CUPS".to_string(),
                (Some(e), _) => e.clone(),
                (None, _) => "Consultando…".to_string(),
            }))
        }
    }
    col = col.push(status);

    let mut queue = settings::section().title("Fila de impressão");
    if app.jobs.is_empty() {
        queue = queue.add(widget::text::body("Fila vazia"));
    }
    for j in &app.jobs {
        let pos = j.queue_position.map(|p| format!("#{p} · ")).unwrap_or_default();
        let info = widget::Column::new()
            .push(widget::text::body(if j.title.is_empty() { format!("Job {}", j.id) } else { j.title.clone() }))
            .push(widget::text::caption(format!("{pos}{} · {} KiB · {}", j.state.label(), j.size_kib, j.user)))
            .width(Length::Fill);
        let mut row = widget::Row::new().spacing(8).align_y(Alignment::Center).push(info);
        if j.state.is_active() {
            row = row.push(widget::tooltip(
                widget::button::icon(widget::icon::from_name("process-stop-symbolic"))
                    .on_press(Message::CancelJob(j.id)),
                widget::text::body("Cancelar"),
                widget::tooltip::Position::Left,
            ));
        }
        queue = queue.add(row);
    }
    col = col.push(queue);

    widget::scrollable(col).width(Length::Fixed(STATUS_WIDTH)).into()
}
