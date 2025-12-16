//! TUI module

mod app;
pub mod highlight;
pub mod icons;
mod input;
mod layout;
pub mod overlays;
pub mod state;
pub mod theme;
pub mod views;
pub mod widgets;

pub use app::{run_with_client, App};
