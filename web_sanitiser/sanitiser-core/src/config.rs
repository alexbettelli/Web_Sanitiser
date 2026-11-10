//! Configurazione dichiarativa delle regole del core.

use url::Url;

#[derive(Default, Debug, Clone, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    pub scripts: ScriptPolicy,
}

impl Policy {
    /// Legge la configurazione da testo TOML, senza accedere al filesystem.
    pub fn from_toml(input: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(input)
    }
}

/// Script HTML autorizzati. Le liste vuote rifiutano tutti gli script.
#[derive(Default, Debug, Clone, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScriptPolicy {
    /// URL assoluti HTTP/HTTPS. Il confronto riguarda l'intero URL normalizzato,
    /// inclusi percorso e query; non vengono accettati prefissi o wildcard.
    pub allowed_sources: Vec<String>,
    /// Contenuti inline esatti, come rappresentati nel DOM dopo il parsing.
    /// Anche gli spazi fanno parte del confronto. Non sono pattern JavaScript.
    pub allowed_inline: Vec<String>,
}

impl ScriptPolicy {
    /// Verifica un riferimento esterno senza scaricarne il contenuto.
    pub fn allows_source(&self, source: &str) -> bool {
        let Some(source) = parse_script_url(source) else {
            return false;
        };

        self.allowed_sources
            .iter()
            .filter_map(|allowed| parse_script_url(allowed))
            .any(|allowed| allowed == source)
    }

    pub fn allows_inline(&self, content: &str) -> bool {
        self.allowed_inline.iter().any(|allowed| allowed == content)
    }
}

fn parse_script_url(value: &str) -> Option<Url> {
    // Richiediamo la forma esplicita schema://autorità: riferimenti come
    // https:app.js possono dipendere dalla base quando letti da un browser.
    value.split_once("://")?;
    let url = Url::parse(value).ok()?;

    // I riferimenti relativi richiederanno il contesto della pagina.
    // Schemi attivi come javascript: e data: non sono autorizzabili qui.
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }

    Some(url)
}

#[cfg(test)]
mod tests {
    use super::Policy;

    #[test]
    fn omitted_configuration_denies_all_scripts() {
        for input in ["", "[scripts]"] {
            let policy = Policy::from_toml(input).unwrap();

            assert!(!policy.scripts.allows_source("https://cdn.example/app.js"));
            assert!(!policy.scripts.allows_inline("trusted()"));
        }
    }

    #[test]
    fn loads_allow_lists_from_toml() {
        let policy = Policy::from_toml(include_str!("../../policy.example.toml")).unwrap();

        assert!(policy
            .scripts
            .allows_source("HTTPS://CDN.EXAMPLE:443/assets/app.js?v=1"));
        assert!(policy.scripts.allows_inline("window.appReady = true;"));
        assert!(!policy.scripts.allows_inline(" window.appReady = true;"));
        assert!(!policy
            .scripts
            .allows_source("https://cdn.example/assets/app.js?v=2"));
    }

    #[test]
    fn rejects_invalid_configuration() {
        for input in [
            "[scripts",
            "[scripts]\nallowed_sources = true",
            "[scripts]\nallow_all = true",
            "[unknown]\nvalue = true",
        ] {
            assert!(Policy::from_toml(input).is_err(), "configurazione: {input}");
        }
    }
}
