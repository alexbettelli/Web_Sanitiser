#[derive(Debug, thiserror::Error)]
pub enum SanitiseError {
    #[error("generic error")]
    GenericError
}