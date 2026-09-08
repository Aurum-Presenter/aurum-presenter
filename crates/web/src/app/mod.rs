//! The shell: what holds the app up regardless of which screen is showing.

pub mod locks;
pub mod shell;
pub mod storage;
pub mod workspace;

pub use workspace::{WorkspaceContext, provide_workspace, use_workspace};
