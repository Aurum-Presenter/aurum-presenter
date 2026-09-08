//! The rules both halves of the app obey.
//!
//! Everything in this crate is a pure function of its arguments: no clock, no network, no
//! database, no DOM. That is what lets it compile natively for the server and to WebAssembly for
//! the browser, and it is the whole point of the crate — a rule that lives here cannot be
//! implemented twice and cannot drift between the two sides of the wire.
#![forbid(unsafe_code)]

pub mod chart;
pub mod present;
pub mod sheets;

/// Proves the crate builds for both targets. Replaced by the first real rule that moves in.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crate_reports_its_version() {
        assert!(!version().is_empty());
    }
}
