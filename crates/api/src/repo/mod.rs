//! Reading and writing the control tier. No rules live here — these are the SQL statements the
//! handlers would otherwise be writing inline.

pub mod accounts;
pub mod invites;
pub mod sessions;
pub mod workspaces;

use crate::db::now_ms;

/// A client-minted id, in the one format every id column in this system uses.
pub fn new_id() -> String {
    use rand::RngCore;

    let mut random = [0_u8; 10];
    rand::rng().fill_bytes(&mut random);

    aurum_core::ids::uuidv7(now_ms(), random)
}

/// A timestamp `n` seconds from now, in the stored format.
pub fn in_seconds(seconds: i64) -> String {
    aurum_core::time::format(now_ms() + seconds * 1000)
}
