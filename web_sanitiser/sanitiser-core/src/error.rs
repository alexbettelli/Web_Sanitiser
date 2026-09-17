//! Tipi di errore condivisi da tutto `sanitizer-core`.
//!
//! Il core è pensato per essere usato anche da altri programmi 
//! quindi gli errori sono tipizzati
//! (`thiserror`) invece di essere semplici stringhe: chi integra la
//! libreria può fare pattern matching e decidere come reagire.

use std::path::PathBuf;

/// Errore generale del core.
///  Ogni sotto-modulo ha una propria variante o un proprio
/// tipo di errore incapsulato qui via `#[from]` / `#[error(transparent)]`.
#[derive(Debug, thiserror::Error)]
pub enum SanitiseError {
    #[error(transparent)]
    Input(#[from] InputError),
}

/// Errori specifici della fase di input
#[derive(Debug, thiserror::Error)]
pub enum InputError {
    #[error("il percorso non esiste: {0}")]
    PathNotFound(PathBuf),

    #[error("impossibile determinare il tipo di input per {0}")]
    UnknownInputKind(PathBuf),

    #[error("errore di I/O su {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("errore durante la scansione della directory {path}: {source}")]
    WalkDir {
        path: PathBuf,
        #[source]
        source: walkdir::Error,
    },

    #[error("URL non valido: {0}")]
    InvalidUrl(#[from] url::ParseError),

    #[error("schema URL non consentito: {scheme} (consentiti: {allowed:?})")]
    SchemeNotAllowed { scheme: String, allowed: Vec<String> },

    #[error("la lista di URL {path} è vuota o non contiene URL validi")]
    EmptyUrlList { path: PathBuf },

    #[error("nessun input fornito")]
    NoInput,

    #[error("errore HTTP durante il fetch di {url}: {source}")]
    Http {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error( "risposta troppo grande per {url}: {actual} byte (limite {limit} byte)" )]
    ResponseTooLarge {
        url: String,
        actual: u64,
        limit: u64,
    },

    #[error("troppi redirect ({count}) per {url} (limite {limit})")]
    TooManyRedirects { url: String, count: u32, limit: u32 },

    #[error("timeout durante il fetch di {url}")]
    Timeout { url: String },
}

pub type InputResult<T> = std::result::Result<T, InputError>;