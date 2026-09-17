//Definisce i tipi di input

use std::path::PathBuf;

pub enum Input {
    File(PathBuf),
    Directory(PathBuf),
    Url(url::Url),
}