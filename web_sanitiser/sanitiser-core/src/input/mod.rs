//! sanitiser-core/input
//!
//! Si iccupa di gestire i tipi di input: file, directory, URL.

pub mod source;
pub mod local;
pub mod remote;

pub use source::Input;