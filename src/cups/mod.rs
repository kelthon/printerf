//! Camada segura sobre a libcups.
//!
//! Todas as funções aqui são **bloqueantes** (a libcups faz I/O HTTP/IPP síncrono).
//! A UI as chama via `tokio::task::spawn_blocking` — veja [`async_api`].

mod ffi;

use std::ffi::{CStr, CString, c_char, c_int};
use std::path::Path;
use std::ptr;

use thiserror::Error;

#[derive(Debug, Error, Clone)]
pub enum CupsError {
    #[error("CUPS: {0}")]
    Cups(String),
    #[error("string inválida (contém NUL): {0}")]
    Nul(String),
    #[error("“{0}” é uma impressora de rede sem fila local: o status aparece depois da primeira impressão")]
    NoLocalQueue(String),
}

pub type Result<T> = std::result::Result<T, CupsError>;

fn cstring(s: &str) -> Result<CString> {
    CString::new(s).map_err(|_| CupsError::Nul(s.to_owned()))
}

unsafe fn opt_str(p: *const c_char) -> Option<String> {
    if p.is_null() { None } else { Some(unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()) }
}

fn last_error() -> CupsError {
    CupsError::Cups(unsafe { opt_str(ffi::cupsLastErrorString()) }.unwrap_or_else(|| "erro desconhecido".into()))
}

// ---------------------------------------------------------------------------
// Impressoras
// ---------------------------------------------------------------------------

/// Estado IPP `printer-state` (RFC 8011 §5.4.11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrinterState {
    Idle,
    Processing,
    Stopped,
    Unknown,
}

impl PrinterState {
    fn from_ipp(v: i32) -> Self {
        match v {
            3 => Self::Idle,
            4 => Self::Processing,
            5 => Self::Stopped,
            _ => Self::Unknown,
        }
    }
}

/// Condição de hardware derivada de `printer-state-reasons`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Condition {
    MediaEmpty,
    MediaJam,
    MarkerLow,
    MarkerEmpty,
    Offline,
    DoorOpen,
    Paused,
    Other(String),
}

impl Condition {
    /// Mapeia uma keyword IPP, removendo sufixos `-report/-warning/-error`.
    pub fn from_reason(reason: &str) -> Option<Self> {
        let base = reason.trim_end_matches("-error").trim_end_matches("-warning").trim_end_matches("-report");
        Some(match base {
            "none" => return None,
            "media-empty" | "media-needed" | "input-tray-missing" => Self::MediaEmpty,
            "media-jam" => Self::MediaJam,
            "marker-supply-low" | "toner-low" | "ink-low" => Self::MarkerLow,
            "marker-supply-empty" | "toner-empty" | "ink-empty" => Self::MarkerEmpty,
            "offline" | "connecting-to-device" | "shutdown" | "timed-out" => Self::Offline,
            "door-open" | "cover-open" => Self::DoorOpen,
            "paused" => Self::Paused,
            other => Self::Other(other.to_owned()),
        })
    }

    pub fn label(&self) -> String {
        match self {
            Self::MediaEmpty => "Sem papel".into(),
            Self::MediaJam => "Papel atolado".into(),
            Self::MarkerLow => "Tinta baixa".into(),
            Self::MarkerEmpty => "Tinta esgotada".into(),
            Self::Offline => "Offline".into(),
            Self::DoorOpen => "Tampa aberta".into(),
            Self::Paused => "Pausada".into(),
            Self::Other(s) => s.clone(),
        }
    }
}

/// Nível de um cartucho/tanque (`marker-*`).
#[derive(Debug, Clone)]
pub struct Marker {
    pub name: String,
    /// 0–100, ou `None` se a impressora reportar -1/-2/-3 (desconhecido).
    pub level: Option<u8>,
}

#[derive(Debug, Clone)]
pub struct Printer {
    pub name: String,
    pub info: Option<String>,
    pub make_and_model: Option<String>,
    pub is_default: bool,
    /// Destino só **descoberto** na rede (DNS-SD): o CUPS cria a fila na hora de
    /// imprimir; até lá não há status nem fila local para consultar.
    pub discovered: bool,
}

impl Printer {
    pub fn is_epson(&self) -> bool {
        let hay = format!("{} {}", self.name, self.make_and_model.as_deref().unwrap_or("")).to_lowercase();
        hay.contains("epson")
    }

    pub fn display_name(&self) -> &str {
        self.info.as_deref().filter(|s| !s.is_empty()).unwrap_or(&self.name)
    }

    /// Rótulo para listas: nome amigável + marcas.
    pub fn label(&self) -> String {
        let mut l = self.display_name().to_owned();
        if self.discovered {
            l.push_str(" (rede)");
        }
        if self.is_default {
            l.push_str(" · padrão");
        }
        l
    }
}

#[derive(Debug, Clone)]
pub struct PrinterStatus {
    pub state: PrinterState,
    pub accepting_jobs: bool,
    pub message: Option<String>,
    pub conditions: Vec<Condition>,
    pub markers: Vec<Marker>,
    /// `media-source-supported` (bandejas), ex.: `auto`, `main`, `rear`.
    pub media_sources: Vec<String>,
    /// `sides-supported`: indica se duplex está disponível.
    pub sides: Vec<String>,
}

impl PrinterStatus {
    pub fn summary(&self) -> String {
        if let Some(c) = self.conditions.iter().find(|c| !matches!(c, Condition::Other(_))) {
            return c.label();
        }
        match self.state {
            PrinterState::Idle if self.accepting_jobs => "Pronta".into(),
            PrinterState::Idle => "Não aceitando trabalhos".into(),
            PrinterState::Processing => "Ocupada (imprimindo)".into(),
            PrinterState::Stopped => "Parada".into(),
            PrinterState::Unknown => "Desconhecido".into(),
        }
    }

    pub fn supports_duplex(&self) -> bool {
        self.sides.iter().any(|s| s.starts_with("two-sided"))
    }
}

pub fn is_discovered_type(printer_type: u32) -> bool {
    printer_type & ffi::CUPS_PRINTER_DISCOVERED != 0
}

/// Lista os destinos configurados no CUPS (impressoras + classes), Epson primeiro.
pub fn list_printers() -> Result<Vec<Printer>> {
    let mut dests: *mut ffi::cups_dest_t = ptr::null_mut();
    let n = unsafe { ffi::cupsGetDests2(ffi::CUPS_HTTP_DEFAULT, &mut dests) };
    if n < 0 {
        return Err(last_error());
    }

    let mut out = Vec::with_capacity(n as usize);
    for i in 0..n as usize {
        let d = unsafe { &*dests.add(i) };
        if !d.instance.is_null() {
            continue; // instâncias (lpoptions) não são impressoras distintas
        }
        let get = |k: &CStr| unsafe { opt_str(ffi::cupsGetOption(k.as_ptr(), d.num_options, d.options)) };
        let printer_type: u32 = get(c"printer-type").and_then(|t| t.parse().ok()).unwrap_or(0);
        out.push(Printer {
            name: unsafe { opt_str(d.name) }.unwrap_or_default(),
            info: get(c"printer-info"),
            make_and_model: get(c"printer-make-and-model"),
            is_default: d.is_default != 0,
            discovered: is_discovered_type(printer_type),
        });
    }
    unsafe { ffi::cupsFreeDests(n, dests) };

    // Epson primeiro; filas locais (com status) antes de destinos só descobertos.
    out.sort_by_key(|p| (!p.is_epson(), p.discovered, !p.is_default, p.name.clone()));
    Ok(out)
}

/// RAII para respostas IPP.
struct Ipp(*mut ffi::ipp_t);
impl Drop for Ipp {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { ffi::ippDelete(self.0) }
        }
    }
}

impl Ipp {
    fn attr(&self, name: &CStr) -> *mut ffi::ipp_attribute_t {
        unsafe { ffi::ippFindAttribute(self.0, name.as_ptr(), ffi::IPP_TAG_ZERO) }
    }
    fn strings(&self, name: &CStr) -> Vec<String> {
        let a = self.attr(name);
        if a.is_null() {
            return vec![];
        }
        (0..unsafe { ffi::ippGetCount(a) })
            .filter_map(|i| unsafe { opt_str(ffi::ippGetString(a, i, ptr::null_mut())) })
            .collect()
    }
    fn ints(&self, name: &CStr) -> Vec<i32> {
        let a = self.attr(name);
        if a.is_null() {
            return vec![];
        }
        (0..unsafe { ffi::ippGetCount(a) }).map(|i| unsafe { ffi::ippGetInteger(a, i) }).collect()
    }
    fn boolean(&self, name: &CStr) -> Option<bool> {
        let a = self.attr(name);
        (!a.is_null()).then(|| unsafe { ffi::ippGetBoolean(a, 0) } != 0)
    }
}

/// Faz `Get-Printer-Attributes` no cupsd local. Diferente de `cupsGetDests`,
/// isso traz níveis de tinta (`marker-levels`) e `printer-state-reasons` atualizados.
pub fn printer_status(name: &str) -> Result<PrinterStatus> {
    let uri = cstring(&format!("ipp://localhost/printers/{name}"))?;
    let requested: [&CStr; 9] = [
        c"printer-state",
        c"printer-state-reasons",
        c"printer-state-message",
        c"printer-is-accepting-jobs",
        c"marker-names",
        c"marker-levels",
        c"media-source-supported",
        c"sides-supported",
        c"printer-make-and-model",
    ];
    let ptrs: Vec<*const c_char> = requested.iter().map(|s| s.as_ptr()).collect();

    let resp = unsafe {
        let req = ffi::ippNewRequest(ffi::IPP_OP_GET_PRINTER_ATTRIBUTES);
        ffi::ippAddString(
            req,
            ffi::IPP_TAG_OPERATION,
            ffi::IPP_TAG_URI,
            c"printer-uri".as_ptr(),
            ptr::null(),
            uri.as_ptr(),
        );
        ffi::ippAddStrings(
            req,
            ffi::IPP_TAG_OPERATION,
            ffi::IPP_TAG_KEYWORD,
            c"requested-attributes".as_ptr(),
            ptrs.len() as c_int,
            ptr::null(),
            ptrs.as_ptr(),
        );
        Ipp(ffi::cupsDoRequest(ffi::CUPS_HTTP_DEFAULT, req, c"/".as_ptr()))
    };
    if resp.0.is_null() {
        return Err(last_error());
    }
    let code = unsafe { ffi::ippGetStatusCode(resp.0) };
    if code >= ffi::IPP_STATUS_ERROR_BAD_REQUEST {
        return Err(if code == ffi::IPP_STATUS_ERROR_NOT_FOUND {
            CupsError::NoLocalQueue(name.to_owned())
        } else {
            last_error()
        });
    }

    let names = resp.strings(c"marker-names");
    let levels = resp.ints(c"marker-levels");
    let markers = names
        .into_iter()
        .enumerate()
        .map(|(i, name)| Marker { name, level: levels.get(i).copied().filter(|l| *l >= 0).map(|l| l.min(100) as u8) })
        .collect();

    Ok(PrinterStatus {
        state: PrinterState::from_ipp(resp.ints(c"printer-state").first().copied().unwrap_or(0)),
        accepting_jobs: resp.boolean(c"printer-is-accepting-jobs").unwrap_or(true),
        message: resp.strings(c"printer-state-message").into_iter().next().filter(|s| !s.is_empty()),
        conditions: resp.strings(c"printer-state-reasons").iter().filter_map(|r| Condition::from_reason(r)).collect(),
        markers,
        media_sources: resp.strings(c"media-source-supported"),
        sides: resp.strings(c"sides-supported"),
    })
}

// ---------------------------------------------------------------------------
// Fila / jobs
// ---------------------------------------------------------------------------

/// `ipp_jstate_t`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Pending,
    Held,
    Processing,
    Stopped,
    Canceled,
    Aborted,
    Completed,
}

impl JobState {
    fn from_ipp(v: c_int) -> Self {
        match v {
            3 => Self::Pending,
            4 => Self::Held,
            5 => Self::Processing,
            6 => Self::Stopped,
            7 => Self::Canceled,
            8 => Self::Aborted,
            _ => Self::Completed,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Pending => "Na fila",
            Self::Held => "Retido",
            Self::Processing => "Imprimindo",
            Self::Stopped => "Parado",
            Self::Canceled => "Cancelado",
            Self::Aborted => "Abortado",
            Self::Completed => "Concluído",
        }
    }
    pub fn is_active(self) -> bool {
        matches!(self, Self::Pending | Self::Held | Self::Processing | Self::Stopped)
    }
}

#[derive(Debug, Clone)]
pub struct Job {
    pub id: i32,
    pub title: String,
    pub user: String,
    pub state: JobState,
    pub size_kib: i32,
    /// Posição 1-based entre os jobs ativos da impressora.
    pub queue_position: Option<usize>,
}

/// Jobs da impressora: ativos (com posição na fila) seguidos dos últimos concluídos.
pub fn list_jobs(printer: &str, include_completed: bool) -> Result<Vec<Job>> {
    let name = cstring(printer)?;
    let which = if include_completed { ffi::CUPS_WHICHJOBS_ALL } else { ffi::CUPS_WHICHJOBS_ACTIVE };
    let mut jobs: *mut ffi::cups_job_t = ptr::null_mut();
    let n = unsafe { ffi::cupsGetJobs2(ffi::CUPS_HTTP_DEFAULT, &mut jobs, name.as_ptr(), 0, which) };
    if n < 0 {
        // Sem fila local (destino descoberto na rede) = nenhum job ainda.
        if unsafe { ffi::cupsLastError() } == ffi::IPP_STATUS_ERROR_NOT_FOUND {
            return Ok(vec![]);
        }
        return Err(last_error());
    }

    let mut out: Vec<Job> = (0..n as usize)
        .map(|i| {
            let j = unsafe { &*jobs.add(i) };
            Job {
                id: j.id,
                title: unsafe { opt_str(j.title) }.unwrap_or_default(),
                user: unsafe { opt_str(j.user) }.unwrap_or_default(),
                state: JobState::from_ipp(j.state),
                size_kib: j.size,
                queue_position: None,
            }
        })
        .collect();
    unsafe { ffi::cupsFreeJobs(n, jobs) };

    // CUPS processa FIFO por id (salvo prioridade); ativos primeiro, depois recentes.
    out.sort_by_key(|j| (!j.state.is_active(), if j.state.is_active() { j.id } else { -j.id }));
    let mut pos = 0;
    for j in out.iter_mut().filter(|j| j.state.is_active()) {
        pos += 1;
        j.queue_position = Some(pos);
    }
    out.truncate(20);
    Ok(out)
}

/// Atributos de job enviados ao CUPS (nomes = opções `lp -o`).
#[derive(Debug, Clone, Default)]
pub struct JobOptions(pub Vec<(String, String)>);

impl JobOptions {
    pub fn set(&mut self, k: impl Into<String>, v: impl Into<String>) -> &mut Self {
        self.0.push((k.into(), v.into()));
        self
    }
}

/// Envia um arquivo para a fila. Retorna o id do job.
pub fn print_file(printer: &str, path: &Path, title: &str, options: &JobOptions) -> Result<i32> {
    let name = cstring(printer)?;
    let file = cstring(&path.to_string_lossy())?;
    let title = cstring(title)?;

    let mut num: c_int = 0;
    let mut opts: *mut ffi::cups_option_t = ptr::null_mut();
    for (k, v) in &options.0 {
        let (k, v) = (cstring(k)?, cstring(v)?);
        num = unsafe { ffi::cupsAddOption(k.as_ptr(), v.as_ptr(), num, &mut opts) };
    }

    let id =
        unsafe { ffi::cupsPrintFile2(ffi::CUPS_HTTP_DEFAULT, name.as_ptr(), file.as_ptr(), title.as_ptr(), num, opts) };
    unsafe { ffi::cupsFreeOptions(num, opts) };

    if id == 0 { Err(last_error()) } else { Ok(id) }
}

pub fn cancel_job(printer: &str, job_id: i32) -> Result<()> {
    let name = cstring(printer)?;
    // ipp_status_t: < 0x0400 é sucesso
    let status = unsafe { ffi::cupsCancelJob2(ffi::CUPS_HTTP_DEFAULT, name.as_ptr(), job_id, 0) };
    if status < 0x0400 { Ok(()) } else { Err(last_error()) }
}

/// Wrappers assíncronos para uso direto em `Task`s da libcosmic.
pub mod async_api {
    use super::*;

    async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
        tokio::task::spawn_blocking(f)
            .await
            .unwrap_or_else(|e| Err(CupsError::Cups(format!("thread de CUPS falhou: {e}"))))
    }

    pub async fn list_printers() -> Result<Vec<Printer>> {
        blocking(super::list_printers).await
    }

    /// Status do hardware + fila em uma única ida ao cupsd.
    pub async fn snapshot(printer: String) -> Result<(PrinterStatus, Vec<Job>)> {
        blocking(move || Ok((super::printer_status(&printer)?, super::list_jobs(&printer, true)?))).await
    }

    pub async fn print_file(printer: String, path: std::path::PathBuf, title: String, opts: JobOptions) -> Result<i32> {
        blocking(move || super::print_file(&printer, &path, &title, &opts)).await
    }

    pub async fn cancel_job(printer: String, id: i32) -> Result<()> {
        blocking(move || super::cancel_job(&printer, id)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasons_map_to_conditions() {
        assert_eq!(Condition::from_reason("media-empty-error"), Some(Condition::MediaEmpty));
        assert_eq!(Condition::from_reason("marker-supply-low-warning"), Some(Condition::MarkerLow));
        assert_eq!(Condition::from_reason("offline-report"), Some(Condition::Offline));
        assert_eq!(Condition::from_reason("none"), None);
        assert_eq!(Condition::from_reason("cover-open-error"), Some(Condition::DoorOpen));
        assert_eq!(Condition::from_reason("com.epson-foo"), Some(Condition::Other("com.epson-foo".into())));
    }

    fn status(state: PrinterState, conditions: Vec<Condition>) -> PrinterStatus {
        PrinterStatus {
            state,
            accepting_jobs: true,
            message: None,
            conditions,
            markers: vec![],
            media_sources: vec![],
            sides: vec!["one-sided".into()],
        }
    }

    #[test]
    fn summary_prefers_known_hardware_conditions() {
        assert_eq!(status(PrinterState::Idle, vec![]).summary(), "Pronta");
        assert_eq!(status(PrinterState::Processing, vec![]).summary(), "Ocupada (imprimindo)");
        let s = status(PrinterState::Idle, vec![Condition::Other("x".into()), Condition::MediaEmpty]);
        assert_eq!(s.summary(), "Sem papel");
        let mut s = status(PrinterState::Idle, vec![]);
        s.accepting_jobs = false;
        assert_eq!(s.summary(), "Não aceitando trabalhos");
    }

    #[test]
    fn duplex_support_from_sides() {
        let mut s = status(PrinterState::Idle, vec![]);
        assert!(!s.supports_duplex());
        s.sides.push("two-sided-long-edge".into());
        assert!(s.supports_duplex());
    }

    #[test]
    fn job_states() {
        assert_eq!(JobState::from_ipp(3), JobState::Pending);
        assert_eq!(JobState::from_ipp(5), JobState::Processing);
        assert_eq!(JobState::from_ipp(9), JobState::Completed);
        assert!(JobState::Held.is_active());
        assert!(!JobState::Canceled.is_active());
        assert_eq!(PrinterState::from_ipp(5), PrinterState::Stopped);
    }

    #[test]
    fn epson_detection_and_display_name() {
        let p = Printer {
            name: "L3250".into(),
            info: Some(String::new()),
            make_and_model: Some("EPSON L3250 Series".into()),
            is_default: false,
            discovered: false,
        };
        assert!(p.is_epson());
        assert_eq!(p.display_name(), "L3250"); // info vazio cai no nome
        let hp = Printer {
            name: "HP".into(),
            info: Some("Escritório".into()),
            make_and_model: None,
            is_default: true,
            discovered: true,
        };
        assert!(!hp.is_epson());
        assert_eq!(hp.display_name(), "Escritório");
        assert_eq!(hp.label(), "Escritório (rede) · padrão");
        assert_eq!(p.label(), "L3250");
    }

    #[test]
    fn discovered_flag_from_printer_type() {
        // Valores reais: destino DNS-SD da L3250 vs. fila do cups-browsed.
        assert!(is_discovered_type(83_890_188));
        assert!(!is_discovered_type(2_134_028));
    }

    #[test]
    fn rejects_nul_in_names() {
        assert!(matches!(print_file("a\0b", Path::new("/x"), "t", &JobOptions::default()), Err(CupsError::Nul(_))));
    }
}
