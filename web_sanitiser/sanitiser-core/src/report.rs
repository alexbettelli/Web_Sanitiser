/// Report strutturato di una singola sanitizzazione.
#[derive(Debug, Default, serde::Serialize)]
pub struct SanitiseReport {
    pub actions: Vec<SanitiseAction>,
}

#[derive(Debug, serde::Serialize)]
pub struct SanitiseAction {
    pub rule: String,
    pub location: String,
    pub original_fragment: String,
    pub replacement: String,
}