//! Aplicação libcosmic (arquitetura Elm: Model → Message → update → view).
//!
//! * `menu`    — barra de menus e atalhos
//! * `tab`     — estado de cada aba (documento, configurações, preview)
//! * `view`    — painel lateral, preview interativo, status/fila
//! * `dialogs` — foto aleatória, atalhos, sobre

mod dialogs;
mod menu;
mod tab;
mod view;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use cosmic::app::{Core, Task};
use cosmic::dialog::file_chooser::{self, FileFilter};
use cosmic::iced::keyboard::{Key, Modifiers, key::Physical};
use cosmic::iced::{Subscription, event, keyboard, mouse};
use cosmic::widget::menu::KeyBind;
use cosmic::widget::segmented_button::{Entity, SingleSelectModel};
use cosmic::{Application, ApplicationExt, Element, executor};
use image::RgbaImage;

use printerf::cups::{self, Job, Printer, PrinterStatus};
use printerf::document::{self, Document, Kind};
use printerf::layout::{
    Crop, FitMode, ItemTransform, NUP_CHOICES, Orientation, PaperSize, PrintSettings, Rotation, SheetGeometry,
    SourcePage,
};
use printerf::portal::{self, PortalOutcome};
use printerf::samples::{self, PhotoSize, PicsumOptions};
use printerf::spool;

use dialogs::{Dialog, PhotoForm, PhotoMsg};
use menu::Action;
use tab::{PREVIEW_DPI, Tab};

pub const APP_ID: &str = "io.github.kelthon.PrinterF";
/// Repositório (Ajuda → Guia de uso). Ajuste se o usuário do GitHub for outro.
pub const REPO_URL: &str = "https://github.com/kelthon/PrinterF";
const POLL_INTERVAL: Duration = Duration::from_secs(2);
const ZOOM_STEP: f32 = 1.1;

#[derive(Debug, Clone, Copy)]
pub enum OpenTarget {
    NewTab,
    Append(Entity),
}

#[derive(Debug, Clone, Copy)]
pub enum CropEdge {
    Left,
    Top,
    Right,
    Bottom,
}

#[derive(Debug, Clone)]
pub enum Message {
    Menu(Action),
    Key(Modifiers, Key, Physical),
    // Arquivos e abas
    FilesChosen(OpenTarget, Vec<PathBuf>),
    DocsLoaded(OpenTarget, Result<Vec<Document>, String>),
    PhotosLoaded(OpenTarget, Box<PicsumOptions>, Result<Document, String>),
    CacheReady(Entity, Result<Arc<Vec<SourcePage>>, String>),
    Composed(Entity, u64, Option<usize>, Result<Vec<Arc<RgbaImage>>, String>),
    TabActivate(Entity),
    TabClose(Entity),
    // Impressoras / fila
    PrintersLoaded(Result<Vec<Printer>, String>),
    SelectPrinter(usize),
    Poll,
    Snapshot(String, Result<(PrinterStatus, Vec<Job>), String>),
    CancelJob(i32),
    JobCancelled(Result<(), String>),
    // Configurações da aba ativa
    Paper(usize),
    Orientation(usize),
    Rotation(usize),
    Fit(usize),
    NUp(usize),
    Copies(i32),
    Duplex(bool),
    Tray(usize),
    PageRange(String),
    Passthrough(bool),
    Repeat(bool),
    // Ajuste livre da imagem selecionada
    Zoom(f32),
    Crop(CropEdge, f32),
    CenterItem,
    ResetItem,
    Deselect,
    PreviewMove(f32, f32),
    PreviewPress,
    PreviewRelease,
    PreviewScroll(f32),
    PrevSheet,
    NextSheet,
    // Impressão
    PrintNative,
    PrintDirect,
    Printed(Result<(i32, usize), String>),
    NativeDone(Result<PortalOutcome, String>),
    // Diálogos e avisos
    OpenPhotoDialog(OpenTarget),
    Photo(PhotoMsg),
    PhotoDownload,
    CloseDialog,
    Notice(String),
    DismissNotice,
}

fn err_str(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

pub struct App {
    core: Core,
    key_binds: HashMap<KeyBind, Action>,

    tabs: SingleSelectModel,

    printers: Vec<Printer>,
    printer_labels: Vec<String>,
    selected_printer: Option<usize>,
    status: Option<PrinterStatus>,
    /// Por que não há status (ex.: impressora de rede sem fila local).
    status_error: Option<String>,
    jobs: Vec<Job>,
    tray_labels: Vec<String>,
    snapshot_in_flight: bool,

    show_aside: bool,
    show_status: bool,
    dialog: Option<Dialog>,
    /// Última posição do cursor no preview (frações da folha).
    hover: Option<(f32, f32)>,
    /// Arrasto em andamento: última posição.
    drag: Option<(f32, f32)>,
    loading: Option<&'static str>,
    busy_printing: bool,
    notice: Option<String>,
}

impl Application for App {
    type Executor = executor::Default;
    type Flags = Vec<PathBuf>;
    type Message = Message;

    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, files: Self::Flags) -> (Self, Task<Message>) {
        let mut app = App {
            core,
            key_binds: menu::key_binds(),
            tabs: SingleSelectModel::default(),
            printers: vec![],
            printer_labels: vec![],
            selected_printer: None,
            status: None,
            status_error: None,
            jobs: vec![],
            tray_labels: vec!["Automática".into()],
            snapshot_in_flight: false,
            show_aside: true,
            show_status: true,
            dialog: None,
            hover: None,
            drag: None,
            loading: None,
            busy_printing: false,
            notice: None,
        };
        // Cabeçalho limpo: só a barra de menus; o nome do documento fica na aba.
        app.set_header_title(String::new());
        let mut tasks = vec![app.load_printers()];
        if let Some(id) = app.core.main_window_id() {
            tasks.push(app.set_window_title("PrinterF".into(), id));
        }
        if !files.is_empty() {
            tasks.push(cosmic::task::message(Message::FilesChosen(OpenTarget::NewTab, files)));
        }
        (app, Task::batch(tasks))
    }

    fn header_start(&self) -> Vec<Element<'_, Message>> {
        let tab = self.active();
        let state = menu::MenuState {
            has_tab: tab.is_some(),
            has_selection: tab.is_some_and(|t| t.selected.is_some()),
            repeat: tab.is_some_and(|t| t.settings.repeat),
            show_aside: self.show_aside,
            show_status: self.show_status,
            printers: &self.printer_labels,
            selected_printer: self.selected_printer,
        };
        vec![menu::bar(&state, &self.key_binds)]
    }

    fn dialog(&self) -> Option<Element<'_, Message>> {
        self.dialog.as_ref().map(|d| d.view(self.active().map(|t| &t.settings)))
    }

    fn on_escape(&mut self) -> Task<Message> {
        if self.dialog.take().is_none() {
            self.with_tab(|t| {
                t.selected = None;
                t.refresh_display();
            });
        }
        Task::none()
    }

    // Subscriptions: polling do cupsd (ADR 0005) + atalhos de teclado.
    fn subscription(&self) -> Subscription<Message> {
        let keys = event::listen_with(|e, status, _| match e {
            cosmic::iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, physical_key, .. })
                if status == event::Status::Ignored =>
            {
                Some(Message::Key(modifiers, key, physical_key))
            }
            _ => None,
        });
        let poll = if self.selected_printer.is_some() {
            cosmic::iced::time::every(POLL_INTERVAL).map(|_| Message::Poll)
        } else {
            Subscription::none()
        };
        Subscription::batch([keys, poll])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Menu(action) => return self.on_menu(action),
            Message::Key(mods, key, physical) => {
                let hit = self.key_binds.iter().find(|(kb, _)| kb.matches(mods, &key, Some(&physical)));
                if let Some((_, action)) = hit {
                    return self.on_menu(*action);
                }
            }

            // ---------------------------------------------------------- arquivos
            Message::FilesChosen(target, paths) => {
                self.loading = Some("Abrindo arquivos…");
                return cosmic::task::future(async move {
                    Message::DocsLoaded(target, document::open_many(paths).await.map_err(err_str))
                });
            }
            Message::DocsLoaded(target, result) => {
                self.loading = None;
                match result {
                    Ok(docs) => {
                        let mut tasks = vec![];
                        for doc in docs {
                            tasks.push(self.add_document(target, doc, PrintSettings::default()));
                        }
                        return Task::batch(tasks);
                    }
                    Err(e) => self.notice = Some(e),
                }
            }
            Message::PhotosLoaded(target, opts, result) => {
                self.loading = None;
                match result {
                    Ok(doc) => {
                        let count = doc.page_count();
                        let mut settings = self.active().map(|t| t.settings.clone()).unwrap_or_default();
                        settings.transforms.clear();
                        settings.page_range.clear();
                        // Uma foto: tamanho real (preset em cm); várias: N-up que caiba todas.
                        settings.fit = match (opts.size, count) {
                            (PhotoSize::FullPage, _) => FitMode::Fill,
                            (_, 1) => FitMode::Original,
                            _ => FitMode::Fit,
                        };
                        if count > 1 {
                            settings.nup = NUP_CHOICES.iter().copied().find(|n| *n as usize >= count).unwrap_or(16);
                        }
                        return self.add_document(target, doc, settings);
                    }
                    Err(e) => self.notice = Some(format!("Falha ao baixar foto: {e}")),
                }
            }
            Message::CacheReady(id, result) => {
                let Some(tab) = self.tabs.data_mut::<Tab>(id) else { return Task::none() };
                match result {
                    Ok(cache) => {
                        tab.cache = Some(cache);
                        return self.recompose(id);
                    }
                    Err(e) => {
                        tab.busy = false;
                        self.notice = Some(format!("Falha ao gerar preview: {e}"));
                    }
                }
            }
            Message::Composed(id, generation, single, result) => {
                let Some(tab) = self.tabs.data_mut::<Tab>(id) else { return Task::none() };
                if generation != tab.generation {
                    return Task::none(); // composição obsoleta
                }
                tab.busy = false;
                match (single, result) {
                    (None, Ok(sheets)) => tab.sheets = sheets,
                    (Some(i), Ok(mut one)) => {
                        if let (Some(slot), Some(img)) = (tab.sheets.get_mut(i), one.pop()) {
                            *slot = img;
                        }
                    }
                    (_, Err(e)) => tab.range_error = Some(e),
                }
                tab.refresh_display();
            }
            Message::TabActivate(id) => {
                self.tabs.activate(id);
                self.drag = None;
            }
            Message::TabClose(id) => {
                self.tabs.remove(id);
                let first = self.tabs.iter().next();
                if let Some(first) = first {
                    self.tabs.activate(first);
                }
            }

            // ---------------------------------------------------------- impressoras
            Message::PrintersLoaded(Ok(printers)) => {
                self.printer_labels = printers.iter().map(Printer::label).collect();
                // `list_printers` já ordena Epson primeiro.
                let keep = self.printer_name().and_then(|n| printers.iter().position(|p| p.name == n));
                self.selected_printer = keep.or((!printers.is_empty()).then_some(0));
                self.printers = printers;
                return self.update(Message::Poll);
            }
            Message::PrintersLoaded(Err(e)) => self.notice = Some(format!("Não foi possível listar impressoras: {e}")),
            Message::SelectPrinter(i) => {
                self.selected_printer = Some(i);
                self.status = None;
                self.status_error = None;
                self.jobs.clear();
                self.with_tab(|t| t.settings.tray = None);
                return self.update(Message::Poll);
            }
            Message::Poll => {
                if self.snapshot_in_flight {
                    return Task::none();
                }
                if let Some(name) = self.printer_name() {
                    self.snapshot_in_flight = true;
                    return cosmic::task::future(async move {
                        let r = cups::async_api::snapshot(name.clone()).await.map_err(err_str);
                        Message::Snapshot(name, r)
                    });
                }
            }
            Message::Snapshot(name, result) => {
                self.snapshot_in_flight = false;
                if Some(&name) != self.printer_name().as_ref() {
                    return Task::none();
                }
                match result {
                    Ok((status, jobs)) => {
                        self.tray_labels = std::iter::once("Automática".to_string())
                            .chain(status.media_sources.iter().filter(|s| *s != "auto").cloned())
                            .collect();
                        self.status = Some(status);
                        self.status_error = None;
                        self.jobs = jobs;
                    }
                    Err(e) => {
                        self.status = None;
                        self.jobs.clear();
                        tracing::debug!("snapshot falhou: {e}");
                        self.status_error = Some(e);
                    }
                }
            }
            Message::CancelJob(id) => {
                if let Some(name) = self.printer_name() {
                    return cosmic::task::future(async move {
                        Message::JobCancelled(cups::async_api::cancel_job(name, id).await.map_err(err_str))
                    });
                }
            }
            Message::JobCancelled(r) => {
                if let Err(e) = r {
                    self.notice = Some(e);
                }
                return self.update(Message::Poll);
            }

            // ---------------------------------------------------------- configurações
            Message::Paper(i) => return self.change(|s| s.paper = PaperSize::ALL[i]),
            Message::Orientation(i) => {
                return self
                    .change(|s| s.orientation = if i == 0 { Orientation::Portrait } else { Orientation::Landscape });
            }
            Message::Rotation(i) => return self.change(|s| s.rotation = Rotation::ALL[i]),
            Message::Fit(i) => return self.change(|s| s.fit = FitMode::ALL[i]),
            Message::NUp(i) => return self.change(|s| s.nup = NUP_CHOICES[i]),
            Message::Repeat(on) => return self.change(|s| s.repeat = on),
            Message::Copies(delta) => {
                self.with_tab(|t| t.settings.copies = (t.settings.copies as i32 + delta).clamp(1, 999) as u16)
            }
            Message::Duplex(on) => self.with_tab(|t| t.settings.duplex = on),
            Message::Tray(i) => {
                let tray = (i > 0).then(|| self.tray_labels[i].clone());
                self.with_tab(|t| t.settings.tray = tray);
            }
            Message::Passthrough(on) => self.with_tab(|t| t.settings.vector_passthrough = on),
            Message::PageRange(text) => {
                let Some(id) = self.active_id() else { return Task::none() };
                let ok = self.tabs.data_mut::<Tab>(id).is_some_and(|t| {
                    t.settings.page_range = text;
                    t.validate_range();
                    t.range_error.is_none()
                });
                if ok {
                    return self.recompose(id);
                }
            }

            // ---------------------------------------------------------- ajuste livre
            Message::Zoom(z) => return self.edit_selected(|t| t.with_zoom(z)),
            Message::Crop(edge, v) => {
                return self.edit_selected(|mut t| {
                    match edge {
                        CropEdge::Left => t.crop.left = v,
                        CropEdge::Top => t.crop.top = v,
                        CropEdge::Right => t.crop.right = v,
                        CropEdge::Bottom => t.crop.bottom = v,
                    }
                    t.crop = t.crop.clamped();
                    t
                });
            }
            Message::CenterItem => {
                return self.edit_selected(|t| ItemTransform { offset_x: 0.0, offset_y: 0.0, ..t });
            }
            Message::ResetItem => return self.edit_selected(|_| ItemTransform::default()),
            Message::Deselect => self.with_tab(|t| {
                t.selected = None;
                t.refresh_display();
            }),
            Message::PreviewMove(fx, fy) => {
                self.hover = Some((fx, fy));
                if let Some((lx, ly)) = self.drag {
                    self.drag = Some((fx, fy));
                    let Some(tab) = self.active() else { return Task::none() };
                    let g = SheetGeometry::new(&tab.settings, PREVIEW_DPI);
                    let (dx, dy) = g.sheet_to_cell_delta(fx - lx, fy - ly);
                    return self.edit_selected(|t| t.moved(dx, dy));
                }
            }
            Message::PreviewPress => {
                let Some((fx, fy)) = self.hover else { return Task::none() };
                let page = self.active().and_then(|t| t.page_at(fx, fy));
                self.drag = page.map(|_| (fx, fy));
                self.with_tab(|t| {
                    t.selected = page;
                    t.refresh_display();
                });
            }
            Message::PreviewRelease => self.drag = None,
            Message::PreviewScroll(lines) => {
                // Zoom na imagem sob o cursor (seleciona-a, se preciso).
                let (Some((fx, fy)), Some(tab)) = (self.hover, self.active()) else { return Task::none() };
                let Some(page) = tab.page_at(fx, fy) else { return Task::none() };
                if tab.selected != Some(page) {
                    self.with_tab(|t| t.selected = Some(page));
                }
                let factor = ZOOM_STEP.powf(lines.clamp(-5.0, 5.0));
                return self.edit_selected(|t| t.with_zoom(t.zoom * factor));
            }
            Message::PrevSheet => self.with_tab(|t| {
                t.sheet = t.sheet.saturating_sub(1);
                t.refresh_display();
            }),
            Message::NextSheet => self.with_tab(|t| {
                t.sheet = (t.sheet + 1).min(t.sheets.len().saturating_sub(1));
                t.refresh_display();
            }),

            // ---------------------------------------------------------- impressão
            Message::PrintNative => {
                let Some(tab) = self.active().filter(|t| t.range_error.is_none()) else { return Task::none() };
                let (doc, settings) = (tab.doc.clone(), tab.settings.clone());
                self.busy_printing = true;
                return cosmic::task::future(async move {
                    let result = async {
                        let title = doc.title();
                        // 1. Diálogo do SO: impressora, cópias, papel, orientação… (ADR 0003)
                        let Some(prepared) = portal::prepare(&title, &settings).await? else {
                            return Ok(PortalOutcome::Cancelled);
                        };
                        // 2. Compõe com papel/orientação escolhidos. 3. O sistema imprime.
                        let settings = portal::apply_choices(&settings, &prepared);
                        let spool = spool::build(doc, settings).await.map_err(err_str)?;
                        portal::print(&spool.path(), &title, prepared.token).await?;
                        Ok(PortalOutcome::Printed)
                    }
                    .await;
                    Message::NativeDone(result)
                });
            }
            Message::PrintDirect => {
                let (Some(tab), Some(printer)) = (self.active(), self.printer_name()) else { return Task::none() };
                if tab.range_error.is_some() {
                    return Task::none();
                }
                let (doc, settings) = (tab.doc.clone(), tab.settings.clone());
                self.busy_printing = true;
                return cosmic::task::future(async move {
                    let result = async {
                        let title = doc.title();
                        let spool = spool::build(doc, settings).await.map_err(err_str)?;
                        let id = cups::async_api::print_file(printer, spool.path(), title, spool.options.clone())
                            .await
                            .map_err(err_str)?;
                        Ok((id, spool.sheets))
                    }
                    .await;
                    Message::Printed(result)
                });
            }
            Message::Printed(r) => {
                self.busy_printing = false;
                self.notice = Some(match r {
                    Ok((id, sheets)) => format!("Job #{id} enviado ({sheets} folha(s))"),
                    Err(e) => format!("Falha ao imprimir: {e}"),
                });
                return self.update(Message::Poll);
            }
            Message::NativeDone(r) => {
                self.busy_printing = false;
                self.notice = match r {
                    Ok(PortalOutcome::Printed) => Some("Enviado pelo diálogo do sistema".into()),
                    Ok(PortalOutcome::Cancelled) => None,
                    Err(e) => Some(format!("{e} — use Impressora → Enviar direto como alternativa")),
                };
            }

            // ---------------------------------------------------------- diálogos
            Message::OpenPhotoDialog(target) => self.dialog = Some(Dialog::Photo(PhotoForm::new(target))),
            Message::Photo(msg) => {
                if let Some(Dialog::Photo(form)) = &mut self.dialog {
                    form.update(msg);
                }
            }
            Message::PhotoDownload => {
                let Some(Dialog::Photo(form)) = self.dialog.take() else { return Task::none() };
                let Ok(opts) = form.options() else {
                    self.notice = form.options().err();
                    self.dialog = Some(Dialog::Photo(form));
                    return Task::none();
                };
                let settings = self.active().map(|t| t.settings.clone()).unwrap_or_default();
                let target = form.target;
                self.loading = Some("Baixando de picsum.photos…");
                return cosmic::task::future(async move {
                    let r = samples::random_photos(&settings, &opts).await.map_err(err_str);
                    Message::PhotosLoaded(target, Box::new(opts), r)
                });
            }
            Message::CloseDialog => self.dialog = None,
            Message::Notice(n) => self.notice = Some(n),
            Message::DismissNotice => self.notice = None,
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        view::main(self)
    }
}

// ---------------------------------------------------------------------------
// Ações de menu
// ---------------------------------------------------------------------------

impl App {
    fn on_menu(&mut self, action: Action) -> Task<Message> {
        let tab = self.active_id();
        match action {
            Action::Open => return pick_files(OpenTarget::NewTab, false),
            Action::AddImages => {
                if let Some(id) = tab.filter(|_| self.active().is_some_and(Tab::is_images)) {
                    return pick_files(OpenTarget::Append(id), true);
                }
                self.notice = Some("“Adicionar imagens” funciona em abas de imagens.".into());
            }
            Action::RandomPhoto => {
                let target = match tab {
                    Some(id) if self.active().is_some_and(Tab::is_images) => OpenTarget::Append(id),
                    _ => OpenTarget::NewTab,
                };
                return self.update(Message::OpenPhotoDialog(target));
            }
            Action::TestPage => {
                self.loading = Some("Gerando página de teste…");
                let (settings, printer) =
                    (self.active().map(|t| t.settings.clone()).unwrap_or_default(), self.printer_name());
                return cosmic::task::future(async move {
                    let r = samples::test_page(&settings, printer).await.map(|d| vec![d]).map_err(err_str);
                    Message::DocsLoaded(OpenTarget::NewTab, r)
                });
            }
            Action::CloseTab => {
                if let Some(id) = tab {
                    return self.update(Message::TabClose(id));
                }
            }
            Action::Quit => return cosmic::iced::exit(),
            Action::RotateLeft | Action::RotateRight => {
                let step = if action == Action::RotateRight { 1 } else { 3 };
                return self.change(|s| {
                    let i = Rotation::ALL.iter().position(|r| *r == s.rotation).unwrap_or(0);
                    s.rotation = Rotation::ALL[(i + step) % 4];
                });
            }
            Action::ZoomIn => return self.edit_selected(|t| t.with_zoom(t.zoom * ZOOM_STEP)),
            Action::ZoomOut => return self.edit_selected(|t| t.with_zoom(t.zoom / ZOOM_STEP)),
            Action::ResetItem => return self.update(Message::ResetItem),
            Action::ResetAll => return self.change(|s| s.transforms.clear()),
            Action::ToggleRepeat => return self.change(|s| s.repeat = !s.repeat),
            Action::Print => return self.update(Message::PrintNative),
            Action::PrintDirect => return self.update(Message::PrintDirect),
            Action::RefreshPrinters => return self.load_printers(),
            Action::SelectPrinter(i) => return self.update(Message::SelectPrinter(i)),
            Action::ToggleAside => self.show_aside = !self.show_aside,
            Action::ToggleStatus => self.show_status = !self.show_status,
            Action::PrevSheet => return self.update(Message::PrevSheet),
            Action::NextSheet => return self.update(Message::NextSheet),
            Action::Guide => {
                let url = format!("{REPO_URL}/blob/main/docs/guides/using.md");
                if let Err(e) = std::process::Command::new("xdg-open").arg(&url).spawn() {
                    self.notice = Some(format!("Não foi possível abrir {url}: {e}"));
                }
            }
            Action::Shortcuts => self.dialog = Some(Dialog::Shortcuts),
            Action::About => self.dialog = Some(Dialog::About),
        }
        Task::none()
    }
}

fn pick_files(target: OpenTarget, images_only: bool) -> Task<Message> {
    cosmic::task::future(async move {
        let mut filter = FileFilter::new(if images_only { "Imagens" } else { "Documentos e imagens" });
        for glob in ["*.png", "*.jpg", "*.jpeg", "*.webp"] {
            filter = filter.glob(glob);
        }
        if !images_only {
            filter = filter.glob("*.pdf").glob("*.txt");
        }
        let dialog = file_chooser::open::Dialog::new().title("Escolha arquivos para imprimir").filter(filter);
        match dialog.open_files().await {
            Ok(r) => {
                let paths: Vec<PathBuf> = r.urls().iter().filter_map(|u| u.to_file_path().ok()).collect();
                if paths.is_empty() {
                    Message::Notice("Somente arquivos locais são suportados".into())
                } else {
                    Message::FilesChosen(target, paths)
                }
            }
            Err(file_chooser::Error::Cancelled) => Message::DismissNotice,
            Err(e) => Message::Notice(format!("Erro no seletor de arquivos: {e}")),
        }
    })
}

// ---------------------------------------------------------------------------
// Helpers de estado
// ---------------------------------------------------------------------------

impl App {
    fn active_id(&self) -> Option<Entity> {
        let id = self.tabs.active();
        self.tabs.data::<Tab>(id).is_some().then_some(id)
    }

    fn active(&self) -> Option<&Tab> {
        self.tabs.data::<Tab>(self.tabs.active())
    }

    fn with_tab(&mut self, f: impl FnOnce(&mut Tab)) {
        let id = self.tabs.active();
        if let Some(t) = self.tabs.data_mut::<Tab>(id) {
            f(t);
        }
    }

    fn printer_name(&self) -> Option<String> {
        self.selected_printer.and_then(|i| self.printers.get(i)).map(|p| p.name.clone())
    }

    fn load_printers(&self) -> Task<Message> {
        cosmic::task::future(async { Message::PrintersLoaded(cups::async_api::list_printers().await.map_err(err_str)) })
    }

    /// Nova aba (ou junta imagens à aba alvo) e dispara a rasterização do preview.
    fn add_document(&mut self, target: OpenTarget, doc: Document, settings: PrintSettings) -> Task<Message> {
        if let OpenTarget::Append(id) = target {
            if let Some(tab) = self.tabs.data_mut::<Tab>(id) {
                if tab.doc.append_images(doc.clone()).is_ok() {
                    // Mais imagens que células? Aumenta o N-up para caber tudo numa folha.
                    let n = tab.doc.page_count();
                    if n > tab.settings.nup as usize && !tab.settings.repeat {
                        tab.settings.nup = NUP_CHOICES.iter().copied().find(|c| *c as usize >= n).unwrap_or(16);
                    }
                    let title = tab.title();
                    self.tabs.text_set(id, title);
                    return self.render_cache(id);
                }
            }
        }
        let mut tab = Tab::new(doc, settings);
        if tab.doc.kind != Kind::Pdf {
            tab.settings.vector_passthrough = false;
        }
        tab.validate_range();
        let title = tab.title();
        let id = self.tabs.insert().text(title).closable().data(tab).activate().id();
        self.render_cache(id)
    }

    /// Rasteriza todas as páginas da aba a PREVIEW_DPI (uma vez por documento).
    fn render_cache(&mut self, id: Entity) -> Task<Message> {
        let Some(tab) = self.tabs.data_mut::<Tab>(id) else { return Task::none() };
        tab.busy = true;
        tab.cache = None;
        let doc = tab.doc.clone();
        cosmic::task::future(async move {
            let all = (0..doc.page_count()).collect();
            let r = document::render_pages(doc, all, PREVIEW_DPI).await.map(Arc::new).map_err(err_str);
            Message::CacheReady(id, r)
        })
    }

    /// Recompõe todas as folhas da aba (mudança de configuração).
    fn recompose(&mut self, id: Entity) -> Task<Message> {
        self.compose(id, None)
    }

    /// `single = Some(folha)`: recompõe só ela (arrasto/zoom/corte).
    fn compose(&mut self, id: Entity, single: Option<usize>) -> Task<Message> {
        let Some(tab) = self.tabs.data_mut::<Tab>(id) else { return Task::none() };
        let Some(cache) = tab.cache.clone() else { return Task::none() };
        if tab.range_error.is_some() {
            return Task::none();
        }
        tab.generation += 1;
        tab.busy = true;
        let (generation, settings) = (tab.generation, tab.settings.clone());
        cosmic::task::future(async move {
            let r = tokio::task::spawn_blocking(move || match single {
                None => spool::compose_cached(&cache, &settings, PREVIEW_DPI),
                Some(i) => {
                    spool::compose_cached_sheet(&cache, &settings, i, PREVIEW_DPI).map(|o| o.into_iter().collect())
                }
            })
            .await
            .map_err(err_str)
            .and_then(|r| r.map_err(err_str))
            .map(|v| v.into_iter().map(Arc::new).collect());
            Message::Composed(id, generation, single, r)
        })
    }

    fn change(&mut self, f: impl FnOnce(&mut PrintSettings)) -> Task<Message> {
        let Some(id) = self.active_id() else { return Task::none() };
        if let Some(t) = self.tabs.data_mut::<Tab>(id) {
            f(&mut t.settings);
            t.validate_range();
        }
        self.recompose(id)
    }

    /// Aplica `f` ao ajuste da página selecionada e recompõe só a folha atual.
    fn edit_selected(&mut self, f: impl FnOnce(ItemTransform) -> ItemTransform) -> Task<Message> {
        let Some(id) = self.active_id() else { return Task::none() };
        let Some(tab) = self.tabs.data_mut::<Tab>(id) else { return Task::none() };
        let Some(page) = tab.selected else { return Task::none() };
        let t = f(tab.settings.transform(page));
        tab.settings.set_transform(page, t);
        let sheet = tab.sheet;
        // Em modo repetir a página aparece em uma folha só; sem ele também.
        self.compose(id, Some(sheet))
    }
}

pub fn run(files: Vec<PathBuf>) -> cosmic::iced::Result {
    let settings = cosmic::app::Settings::default().size(cosmic::iced::Size::new(1360.0, 860.0));
    cosmic::app::run::<App>(settings, files)
}

// Usados por `view`/`dialogs`.
pub(crate) use mouse::ScrollDelta;
pub(crate) const CROP_MAX: f32 = Crop::MAX;
