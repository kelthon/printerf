//! Bindings mínimos e escritos à mão para a libcups 2.x (`<cups/cups.h>`, `<cups/ipp.h>`).
//!
//! Por que não `cups-sys`? A crate está parada há anos e gera bindings via bindgen
//! (exige libclang no build). A superfície que usamos é pequena e estável na ABI
//! desde CUPS 1.6, então declará-la aqui é mais simples e auditável.

#![allow(non_camel_case_types, dead_code)]

use std::ffi::{c_char, c_int, c_void};

pub type http_t = c_void;
pub type ipp_t = c_void;
pub type ipp_attribute_t = c_void;
pub type time_t = i64;

/// `CUPS_HTTP_DEFAULT`: conexão padrão com o cupsd local.
pub const CUPS_HTTP_DEFAULT: *mut http_t = std::ptr::null_mut();

#[repr(C)]
pub struct cups_option_t {
    pub name: *mut c_char,
    pub value: *mut c_char,
}

#[repr(C)]
pub struct cups_dest_t {
    pub name: *mut c_char,
    pub instance: *mut c_char,
    pub is_default: c_int,
    pub num_options: c_int,
    pub options: *mut cups_option_t,
}

#[repr(C)]
pub struct cups_job_t {
    pub id: c_int,
    pub dest: *mut c_char,
    pub title: *mut c_char,
    pub user: *mut c_char,
    pub format: *mut c_char,
    pub state: c_int, // ipp_jstate_t
    pub size: c_int,  // KiB
    pub priority: c_int,
    pub completed_time: time_t,
    pub creation_time: time_t,
    pub processing_time: time_t,
}

// cups_ptype_t: destino descoberto por DNS-SD, ainda sem fila local.
pub const CUPS_PRINTER_DISCOVERED: u32 = 0x0100_0000;
// ipp_status_t
pub const IPP_STATUS_ERROR_NOT_FOUND: c_int = 0x0406;
pub const IPP_STATUS_ERROR_BAD_REQUEST: c_int = 0x0400;

// whichjobs
pub const CUPS_WHICHJOBS_ALL: c_int = -1;
pub const CUPS_WHICHJOBS_ACTIVE: c_int = 0;

// ipp_op_t
pub const IPP_OP_GET_PRINTER_ATTRIBUTES: c_int = 0x000B;
// ipp_tag_t
pub const IPP_TAG_ZERO: c_int = 0x00;
pub const IPP_TAG_OPERATION: c_int = 0x01;
pub const IPP_TAG_KEYWORD: c_int = 0x44;
pub const IPP_TAG_URI: c_int = 0x45;

unsafe extern "C" {
    pub fn cupsGetDests2(http: *mut http_t, dests: *mut *mut cups_dest_t) -> c_int;
    pub fn cupsFreeDests(num_dests: c_int, dests: *mut cups_dest_t);
    pub fn cupsGetOption(name: *const c_char, num_options: c_int, options: *mut cups_option_t) -> *const c_char;
    pub fn cupsAddOption(
        name: *const c_char,
        value: *const c_char,
        num_options: c_int,
        options: *mut *mut cups_option_t,
    ) -> c_int;
    pub fn cupsFreeOptions(num_options: c_int, options: *mut cups_option_t);

    pub fn cupsPrintFile2(
        http: *mut http_t,
        name: *const c_char,
        filename: *const c_char,
        title: *const c_char,
        num_options: c_int,
        options: *mut cups_option_t,
    ) -> c_int;
    pub fn cupsGetJobs2(
        http: *mut http_t,
        jobs: *mut *mut cups_job_t,
        name: *const c_char,
        myjobs: c_int,
        whichjobs: c_int,
    ) -> c_int;
    pub fn cupsFreeJobs(num_jobs: c_int, jobs: *mut cups_job_t);
    pub fn cupsCancelJob2(http: *mut http_t, name: *const c_char, job_id: c_int, purge: c_int) -> c_int;
    pub fn cupsLastErrorString() -> *const c_char;
    /// `ipp_status_t` da última operação.
    pub fn cupsLastError() -> c_int;
    pub fn ippGetStatusCode(ipp: *mut ipp_t) -> c_int;

    pub fn ippNewRequest(op: c_int) -> *mut ipp_t;
    pub fn ippAddString(
        ipp: *mut ipp_t,
        group: c_int,
        value_tag: c_int,
        name: *const c_char,
        language: *const c_char,
        value: *const c_char,
    ) -> *mut ipp_attribute_t;
    pub fn ippAddStrings(
        ipp: *mut ipp_t,
        group: c_int,
        value_tag: c_int,
        name: *const c_char,
        num_values: c_int,
        language: *const c_char,
        values: *const *const c_char,
    ) -> *mut ipp_attribute_t;
    /// Consome (libera) `request`; o retorno deve ser liberado com `ippDelete`.
    pub fn cupsDoRequest(http: *mut http_t, request: *mut ipp_t, resource: *const c_char) -> *mut ipp_t;
    pub fn ippFindAttribute(ipp: *mut ipp_t, name: *const c_char, value_tag: c_int) -> *mut ipp_attribute_t;
    pub fn ippGetCount(attr: *mut ipp_attribute_t) -> c_int;
    pub fn ippGetInteger(attr: *mut ipp_attribute_t, element: c_int) -> c_int;
    pub fn ippGetString(attr: *mut ipp_attribute_t, element: c_int, language: *mut *const c_char) -> *const c_char;
    pub fn ippGetBoolean(attr: *mut ipp_attribute_t, element: c_int) -> c_int;
    pub fn ippDelete(ipp: *mut ipp_t);
}
