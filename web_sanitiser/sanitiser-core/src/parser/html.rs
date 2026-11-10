//! Parsing e serializzazione di documenti HTML tramite `scraper`.

use std::str::Utf8Error;

use scraper::Html;

/// Costruisce il DOM di un documento HTML codificato in UTF-8.
///byte HTML -> DOM
/// I byte non validi in UTF-8 producono un errore. L'HTML malformato viene
/// invece recuperato dal parser secondo le regole HTML5.
/// Questa funzione costruisce il documento senza applicare sanitizzazione.
pub fn parse_html(input: &[u8]) -> Result<Html, Utf8Error> {
    let html = std::str::from_utf8(input)?;

    Ok(Html::parse_document(html))
}

/// Converte il DOM in byte HTML codificati in UTF-8.
///DOM -> byte HTML
/// La serializzazione segue la struttura attuale del DOM: non conserva
/// necessariamente la formattazione originale e non applica sanitizzazione.
pub fn serialize_html(document: &Html) -> Vec<u8> {
    document.html().into_bytes()
}

#[cfg(test)]
mod tests {
    use super::{parse_html, serialize_html};

    #[test]
    fn parses_html_with_implicit_closing_tags() {
        let input = b"<p onclick=\"evil()\">Primo<p>Secondo";

        let document = parse_html(input).expect("l'input deve essere UTF-8 valido");

        assert_eq!(
            document.html(),
            "<html><head></head><body><p onclick=\"evil()\">Primo</p><p>Secondo</p></body></html>"
        );
    }

    #[test]
    fn rejects_invalid_utf8() {
        let input = b"<p>\xff</p>";

        assert!(parse_html(input).is_err());
    }

    #[test]
    fn serializes_utf8_text_and_html_entities() {
        let input = r#"<!DOCTYPE html><p title="Città &amp; mare">Caffè &lt;caldo&gt;</p>"#;
        let document = parse_html(input.as_bytes()).expect("l'input deve essere UTF-8 valido");

        let output = serialize_html(&document);

        let expected = concat!(
            "<!DOCTYPE html><html><head></head><body>",
            "<p title=\"Città &amp; mare\">Caffè &lt;caldo&gt;</p>",
            "</body></html>"
        );
        assert_eq!(output, expected.as_bytes());
    }
}
