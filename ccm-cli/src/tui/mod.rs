//! TUI module

mod app;
pub mod highlight;
mod input;
pub mod navigation;
pub mod state;
mod ui;

pub use app::{run_with_client, App};
