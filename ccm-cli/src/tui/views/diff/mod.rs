//! Diff view - file diffs with word-level highlighting

pub mod input;
pub mod render;

pub use input::handle_diff_files_mode_sync;
pub use render::{draw_diff_fullscreen, draw_diff_view};
