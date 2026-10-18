//! Barra de menus (Arquivo · Editar · Impressora · Exibir · Ajuda) e atalhos.
//!
//! A barra só **exibe** os atalhos; quem os dispara é a subscription de teclado
//! em `App::subscription`, que procura a tecla em [`key_binds`].

use std::collections::HashMap;

use cosmic::Element;
use cosmic::iced::keyboard::{Key, key::Named};
use cosmic::widget::RcElementWrapper;
use cosmic::widget::menu::{self, ItemHeight, ItemWidth, KeyBind, key_bind::Modifier};

use super::Message;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    // Arquivo
    Open,
    AddImages,
    RandomPhoto,
    TestPage,
    CloseTab,
    Quit,
    // Editar
    RotateLeft,
    RotateRight,
    ZoomIn,
    ZoomOut,
    ResetItem,
    ResetAll,
    ToggleRepeat,
    // Impressora
    Print,
    PrintDirect,
    RefreshPrinters,
    SelectPrinter(usize),
    // Exibir
    ToggleAside,
    ToggleStatus,
    PrevSheet,
    NextSheet,
    // Ajuda
    Guide,
    Shortcuts,
    About,
}

impl menu::Action for Action {
    type Message = Message;
    fn message(&self) -> Message {
        Message::Menu(*self)
    }
}

/// Estado necessário para montar os menus (checkboxes, itens habilitados).
pub struct MenuState<'a> {
    pub has_tab: bool,
    pub has_selection: bool,
    pub repeat: bool,
    pub show_aside: bool,
    pub show_status: bool,
    pub printers: &'a [String],
    pub selected_printer: Option<usize>,
}

fn icon(name: &'static str) -> Option<cosmic::widget::icon::Handle> {
    Some(cosmic::widget::icon::from_name(name).into())
}

pub fn bar<'a>(st: &MenuState<'_>, keys: &HashMap<KeyBind, Action>) -> Element<'a, Message> {
    use menu::Item::{Button, ButtonDisabled, CheckBox, Divider, Folder};
    let when = |enabled: bool, label: &'static str, ic, action| {
        if enabled { Button(label.into(), icon(ic), action) } else { ButtonDisabled(label.into(), icon(ic), action) }
    };

    let file = vec![
        Button("Abrir…".into(), icon("document-open-symbolic"), Action::Open),
        when(st.has_tab, "Adicionar imagens à aba…", "list-add-symbolic", Action::AddImages),
        Divider,
        Button("Foto aleatória…".into(), icon("image-x-generic-symbolic"), Action::RandomPhoto),
        Button("Página de teste".into(), icon("document-print-preview-symbolic"), Action::TestPage),
        Divider,
        when(st.has_tab, "Fechar aba", "window-close-symbolic", Action::CloseTab),
        Button("Sair".into(), icon("application-exit-symbolic"), Action::Quit),
    ];

    let edit = vec![
        when(st.has_tab, "Girar à esquerda", "object-rotate-left-symbolic", Action::RotateLeft),
        when(st.has_tab, "Girar à direita", "object-rotate-right-symbolic", Action::RotateRight),
        Divider,
        when(st.has_selection, "Aumentar imagem", "zoom-in-symbolic", Action::ZoomIn),
        when(st.has_selection, "Diminuir imagem", "zoom-out-symbolic", Action::ZoomOut),
        when(st.has_selection, "Redefinir imagem selecionada", "edit-undo-symbolic", Action::ResetItem),
        when(st.has_tab, "Redefinir todos os ajustes", "edit-clear-all-symbolic", Action::ResetAll),
        Divider,
        CheckBox("Repetir em todas as células".into(), icon("view-grid-symbolic"), st.repeat, Action::ToggleRepeat),
    ];

    let mut choose: Vec<menu::Item<Action, String>> = st
        .printers
        .iter()
        .enumerate()
        .map(|(i, name)| CheckBox(name.clone(), None, st.selected_printer == Some(i), Action::SelectPrinter(i)))
        .collect();
    if choose.is_empty() {
        choose.push(ButtonDisabled("Nenhuma impressora no CUPS".into(), None, Action::RefreshPrinters));
    }
    let printer = vec![
        when(st.has_tab, "Imprimir…", "document-print-symbolic", Action::Print),
        when(st.has_tab && st.selected_printer.is_some(), "Enviar direto", "send-to-symbolic", Action::PrintDirect),
        Divider,
        Folder("Impressora para envio direto e status".into(), choose),
        Button("Atualizar impressoras".into(), icon("view-refresh-symbolic"), Action::RefreshPrinters),
    ];

    let view = vec![
        CheckBox("Painel de configurações".into(), icon("sidebar-show-symbolic"), st.show_aside, Action::ToggleAside),
        CheckBox(
            "Painel de status e fila".into(),
            icon("sidebar-show-right-symbolic"),
            st.show_status,
            Action::ToggleStatus,
        ),
        Divider,
        when(st.has_tab, "Folha anterior", "go-previous-symbolic", Action::PrevSheet),
        when(st.has_tab, "Próxima folha", "go-next-symbolic", Action::NextSheet),
    ];

    let help = vec![
        Button("Guia de uso".into(), icon("help-browser-symbolic"), Action::Guide),
        Button("Atalhos de teclado".into(), icon("input-keyboard-symbolic"), Action::Shortcuts),
        Divider,
        Button("Sobre o PrinterF".into(), icon("help-about-symbolic"), Action::About),
    ];

    let root = |label: &'static str, items: Vec<menu::Item<Action, String>>| {
        menu::Tree::with_children(RcElementWrapper::new(Element::from(menu::root(label))), menu::items(keys, items))
    };
    menu::bar(vec![
        root("Arquivo", file),
        root("Editar", edit),
        root("Impressora", printer),
        root("Exibir", view),
        root("Ajuda", help),
    ])
    .item_height(ItemHeight::Dynamic(40))
    .item_width(ItemWidth::Uniform(300))
    .spacing(4.0)
    .into()
}

/// Atalhos (também listados no diálogo “Atalhos de teclado”).
pub fn key_binds() -> HashMap<KeyBind, Action> {
    let mut k = HashMap::new();
    let mut bind = |mods: &[Modifier], key: Key, action| {
        k.insert(KeyBind { modifiers: mods.to_vec(), key }, action);
    };
    let ch = |c: &str| Key::Character(c.into());
    use Modifier::{Ctrl, Shift};

    bind(&[Ctrl], ch("o"), Action::Open);
    bind(&[Ctrl, Shift], ch("o"), Action::AddImages);
    bind(&[Ctrl], ch("r"), Action::RandomPhoto);
    bind(&[Ctrl], ch("t"), Action::TestPage);
    bind(&[Ctrl], ch("w"), Action::CloseTab);
    bind(&[Ctrl], ch("q"), Action::Quit);
    bind(&[Ctrl], ch("["), Action::RotateLeft);
    bind(&[Ctrl], ch("]"), Action::RotateRight);
    bind(&[Ctrl], ch("="), Action::ZoomIn);
    bind(&[Ctrl], ch("-"), Action::ZoomOut);
    bind(&[Ctrl], ch("0"), Action::ResetItem);
    bind(&[Ctrl], ch("p"), Action::Print);
    bind(&[Ctrl, Shift], ch("p"), Action::PrintDirect);
    bind(&[], Key::Named(Named::F5), Action::RefreshPrinters);
    bind(&[], Key::Named(Named::F9), Action::ToggleAside);
    bind(&[], Key::Named(Named::PageUp), Action::PrevSheet);
    bind(&[], Key::Named(Named::PageDown), Action::NextSheet);
    bind(&[], Key::Named(Named::F1), Action::Guide);
    k
}

/// Linhas do diálogo de atalhos: (tecla, descrição).
pub const SHORTCUTS: &[(&str, &str)] = &[
    ("Ctrl+O", "Abrir arquivos (várias imagens viram uma aba)"),
    ("Ctrl+Shift+O", "Adicionar imagens à aba atual"),
    ("Ctrl+R", "Foto aleatória (picsum.photos)"),
    ("Ctrl+T", "Página de teste"),
    ("Ctrl+P", "Imprimir pelo diálogo do sistema"),
    ("Ctrl+Shift+P", "Enviar direto para a impressora selecionada"),
    ("Ctrl+[ / Ctrl+]", "Girar à esquerda / direita"),
    ("Ctrl+= / Ctrl+-", "Aumentar / diminuir imagem selecionada"),
    ("Ctrl+0", "Redefinir imagem selecionada"),
    ("Arrastar no preview", "Mover a imagem selecionada"),
    ("Roda do mouse no preview", "Zoom da imagem sob o cursor"),
    ("PageUp / PageDown", "Folha anterior / próxima"),
    ("F5", "Atualizar impressoras"),
    ("F9", "Mostrar/ocultar painel de configurações"),
    ("Ctrl+W", "Fechar aba"),
    ("F1", "Guia de uso"),
];
