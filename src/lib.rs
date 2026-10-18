//! PrinterF — núcleo sem GUI: integração CUPS/IPP, portal XDG, leitura de
//! documentos, composição de páginas e geração do spool.
//!
//! O binário (`src/main.rs` + `src/app.rs`) é só a camada libcosmic por cima
//! desta biblioteca; tudo aqui é testável sem abrir janela.

pub mod cups;
pub mod document;
pub mod layout;
pub mod portal;
pub mod samples;
pub mod spool;
