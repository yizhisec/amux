//! Git status view - file status panel

pub mod input;
pub mod render;

pub use input::handle_git_status_input_sync;
pub use render::draw_git_status_panel;
