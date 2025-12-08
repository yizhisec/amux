//! Terminal view - embedded shell

pub mod input;
pub mod render;

pub use input::{handle_insert_mode_sync, handle_terminal_normal_mode_sync};
pub use render::{draw_terminal, draw_terminal_fullscreen};
