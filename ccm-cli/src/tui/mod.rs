//! TUI module

mod app;
pub mod highlight;
mod input;
pub mod overlays;
pub mod state;
pub mod tab_bar;
mod ui;
pub mod views;
pub mod widgets;

pub use app::{run_with_client, App};
