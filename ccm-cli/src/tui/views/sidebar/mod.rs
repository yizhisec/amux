//! Sidebar view - repository/branch/session navigation

pub mod input;
pub mod render;

// Re-export commonly used items
pub use input::handle_navigation_input_sync;
pub use render::draw_sidebar;
