//! The screens that are about the workspace and the device rather than about the music.

pub mod about;
pub mod account;
pub mod conflicts;
pub mod members;
pub mod storage;
pub mod sync_panel;

pub use about::AboutPage;
pub use account::AccountPage;
pub use conflicts::ConflictsPage;
pub use members::{InvitePage, MembersPage};
pub use storage::StoragePage;
pub use sync_panel::SyncPanel;
