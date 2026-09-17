//! sanitiser-core
//!
//! Engine di sanitizzazione: non conosce terminale, argv o stdout.
//! Espone solo API pubbliche: input -> (output sanitizzato, report).

pub mod config;
pub mod report;
pub mod error;

pub use error::SanitiseError;
pub use report::SanitiseReport;

/// Punto di ingresso pubblico dell'engine (stub iniziale).
pub fn sanitise(_input: &[u8], _policy: &config::Policy) -> Result<(Vec<u8>, SanitiseReport), SanitiseError> {
    todo!("collegare parsing HTML + regole di policy")
}