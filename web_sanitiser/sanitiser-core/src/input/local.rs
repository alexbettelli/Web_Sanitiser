//! Gestione degli input locali: singoli file e alberi di directory.
//!
//! Si occupa solo di *trovare* e *leggere* i byte da sanificare
//! Non fa parsinge non applica policy

use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::error::{InputError, InputResult};

/// Tipo di asset dedotto dall'estensione del file.
/// Serve a instradare il contenuto verso la sanitizzazione specifica per tipo (fase 2/5
/// del progetto: HTML, CSS, JS, immagini).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssetKind {
    Html,
    Css,
    JavaScript,
    Image,
    /// Qualunque altro file trovato nell'albero (font, json, txt...).
    /// Viene comunque copiato/riportato, ma non sanitizzato per
    /// contenuto.
    Other,
}

impl AssetKind {
    /// Deduce il tipo di asset dall'estensione del path. La scelta è
    /// volutamente semplice (basata sull'estensione, non sul
    /// contenuto/MIME sniffing) per la Fase 1; può essere raffinata in
    /// seguito senza cambiare l'interfaccia pubblica.
    pub fn from_path(path: &Path) -> Self {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        match ext.as_str() {
            "html" | "htm" | "xhtml" => AssetKind::Html,
            "css" => AssetKind::Css,
            "js" | "mjs" | "cjs" => AssetKind::JavaScript,
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "ico" | "bmp" => {
                AssetKind::Image
            }
            _ => AssetKind::Other,
        }
    }
}

/// Un file locale individuato come input, insieme al tipo di asset
/// dedotto e al percorso relativo alla radice della scansione (utile
/// per ricostruire l'albero in output e per il campo `location` nei
/// report).
#[derive(Debug, Clone)]
pub struct LocalFile {
    /// Percorso assoluto/effettivo del file sul filesystem.
    pub absolute_path: PathBuf,
    /// Percorso relativo alla radice dell'input (per un singolo file
    /// coincide col nome del file; per una directory è il path
    /// relativo dentro l'albero).
    pub relative_path: PathBuf,
    pub kind: AssetKind,
}

/// Legge il contenuto grezzo (byte) di un singolo file.
///
/// Non impone limiti di dimensione: i file locali sono per
/// definizione "fidati quanto il filesystem dell'utente"; i limiti
/// (`max_bytes`, `max_requests`, ...) si applicano invece al fetching
/// remoto, dove il contenuto proviene da una sorgente non fidata.
pub fn read_file(path: &Path) -> InputResult<Vec<u8>> {
    std::fs::read(path).map_err(|source| InputError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Costruisce il [`LocalFile`] corrispondente a un singolo file di
/// input (`Input::File`).
pub fn single_file(path: &Path) -> InputResult<LocalFile> {
    if !path.is_file() {
        return Err(InputError::PathNotFound(path.to_path_buf()));
    }
    let file_name = path
        .file_name()
        .map(PathBuf::from)
        .unwrap_or_else(|| path.to_path_buf());

    Ok(LocalFile {
        absolute_path: path.to_path_buf(),
        relative_path: file_name,
        kind: AssetKind::from_path(path),
    })
}

/// Scansiona ricorsivamente una directory (`Input::Directory`) e
/// ritorna tutti i file trovati, con il tipo di asset dedotto e il
/// percorso relativo alla radice `root`.
///
/// File e directory nascosti (che iniziano con `.`) vengono
/// ignorati, così come non si seguono i symlink, per evitare loop e
/// per non uscire accidentalmente dall'albero previsto.
pub fn walk_directory(root: &Path) -> InputResult<Vec<LocalFile>> {
    if !root.is_dir() {
        return Err(InputError::PathNotFound(root.to_path_buf()));
    }

    let mut files = Vec::new();

    let walker = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            e.file_name()
                .to_str()
                .map(|name| name == "." || !name.starts_with('.'))
                .unwrap_or(true)
        });

    for entry in walker {
        let entry = entry.map_err(|source| InputError::WalkDir {
            path: root.to_path_buf(),
            source,
        })?;

        if !entry.file_type().is_file() {
            continue;
        }

        let absolute_path = entry.path().to_path_buf();
        let relative_path = absolute_path
            .strip_prefix(root)
            .unwrap_or(&absolute_path)
            .to_path_buf();

        files.push(LocalFile {
            kind: AssetKind::from_path(&absolute_path),
            absolute_path,
            relative_path,
        });
    }

    // Ordine deterministico: utile per output/report riproducibili e
    // per i test.
    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));

    Ok(files)
}
