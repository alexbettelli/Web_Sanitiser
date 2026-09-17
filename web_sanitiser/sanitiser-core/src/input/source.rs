//! Definisce cosa può essere passato al sanitiser come input e come
//! interpretare gli argomenti "grezzi" ricevuti dalla CLI (o da un
//! programma che integra la libreria).
//!
//! Tre forme di input sono supportate, come da specifica:
//!
//! - un singolo file (HTML o asset);
//! - una directory (albero di un sito);
//! - un file di testo contenente una lista di URL, che viene espanso in più [`Input::Url`].
//!
//! La classificazione è "best effort" e non fa I/O di rete: un URL
//! passato direttamente su riga di comando viene riconosciuto senza
//! toccare il filesystem, mentre un path locale viene ispezionato per
//! distinguere file singoli, directory e liste di URL.

use std::fs;
use std::path::{Path, PathBuf};
use url::Url;

use crate::error::{InputError, InputResult};

/// Un singolo input da elaborare attraverso la pipeline di sanitizzazione.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    File(PathBuf),
    Directory(PathBuf),
    Url(Url),
}

impl Input {
    /// Rappresentazione leggibile dell'input, usata per log e report
    /// (campo `"input"` del JSON di output).
    pub fn display_name(&self) -> String {
        match self {
            Input::File(p) | Input::Directory(p) => p.display().to_string(),
            Input::Url(u) => u.to_string(),
        }
    }
}

/// Risolve una lista di argomenti "grezzi" in una lista piatta di [`Input`].
///
/// Ogni argomento può espandersi in più di un [`Input`]: è il caso di
/// un file che contiene una lista di URL (`urls.txt`), che genera un
/// [`Input::Url`] per ogni riga valida.
///
/// Il batch mode (`web-sanitizer file1.html file2.html file3.html`) è
/// supportato semplicemente passando più argomenti: ognuno viene
/// risolto indipendentemente e i risultati vengono concatenati.
pub fn resolve_args<I, S>(args: I) -> InputResult<Vec<Input>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut resolved = Vec::new();
    for raw in args {
        resolved.extend(classify_arg(raw.as_ref())?);
    }

    if resolved.is_empty() {
        return Err(InputError::NoInput);
    }

    Ok(resolved)
}

/// Classifica un singolo argomento, eventualmente espandendolo in più
/// [`Input`] (caso della lista di URL).
fn classify_arg(raw: &str) -> InputResult<Vec<Input>> {
    // 1. Se l'argomento è già un URL assoluto valido (es. passato
    //    direttamente su riga di comando), non tocchiamo il filesystem.
    if let Ok(url) = Url::parse(raw) {
        if is_supported_scheme(&url) {
            return Ok(vec![Input::Url(url)]);
        }
    }


    // 2. Altrimenti trattiamo l'argomento come un path locale.
    let path = PathBuf::from(raw);
    if !path.exists() {
        return Err(InputError::PathNotFound(path));
    }

    if path.is_dir() {
        return Ok(vec![Input::Directory(path)]);
    }

    if path.is_file() {
        // Potrebbe essere un file "normale" (HTML/asset) oppure una
        // lista di URL, uno per riga. Proviamo a interpretarlo come
        // lista di URL; se non lo è, ricade su Input::File.
        if let Some(urls) = try_parse_url_list(&path)? {
            return Ok(urls.into_iter().map(Input::Url).collect());
        }
        return Ok(vec![Input::File(path)]);
    }

    Err(InputError::UnknownInputKind(path))
}

/// Prova a interpretare `path` come un file di testo con un URL per riga 
/// Righe vuote e righe che iniziano con `#` sono ignorate
///
/// Ritorna:
/// - `Ok(Some(urls))` se il file contiene **solo** righe vuote/commenti
///   e URL assoluti validi, con almeno un URL;
/// - `Ok(None)` se il file non sembra una lista di URL (es. è HTML),
///   nel qual caso il chiamante lo tratterà come `Input::File`;
/// - `Err(_)` in caso di errore di I/O nella lettura.
fn try_parse_url_list(path: &Path) -> InputResult<Option<Vec<Url>>> {
    let content = fs::read_to_string(path).map_err(|source| InputError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    let mut urls = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match Url::parse(line) {
            Ok(url) if is_supported_scheme(&url) => urls.push(url),
            // Anche una sola riga che non è un URL valido basta a
            // dire "questo non è un file di lista URL": lo lasciamo
            // gestire come file normale (es. un file .html che per
            // caso inizia con testo non-URL).
            _ => return Ok(None),
        }
    }

    if urls.is_empty() {
        // File vuoto o con soli commenti: non è una lista di URL utile.
        return Ok(None);
    }

    Ok(Some(urls))
}

fn is_supported_scheme(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
}

