//! The library: folders, songs, and finding one.

pub mod folder_tree;
pub mod import;
pub mod page;
pub mod repository;
pub mod search;
pub mod trash;

pub use repository::{FolderSongs, Library, SongInput};
