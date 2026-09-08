//! The rules the two halves of the app must agree on exactly, or the wire stops meaning
//! anything: which columns a client may write, and what happens when two devices wrote the same
//! row.

pub mod merge;
pub mod schema;
