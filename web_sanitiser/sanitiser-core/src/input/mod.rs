//! Modulo di input: ottiene i contenuti da sanitizzare, che siano
//! file locali, directory o risorse remote.
//!
//! Layout (rispecchia la struttura del progetto):
//!
//! - [`source`]: cos'è un [`Input`](source::Input) e come classificare
//!   gli argomenti "grezzi" passati alla CLI (o a un chiamante della
//!   libreria) in una lista di `Input`.
//! - [`local`]: lettura di file singoli e scansione di alberi di
//!   directory.

pub mod local;
pub mod source;
mod remote;

pub use local::{read_file, single_file, walk_directory, AssetKind, LocalFile};
pub use source::{resolve_args, Input};