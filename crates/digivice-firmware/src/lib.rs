#![no_std]
#![forbid(unsafe_code)]
#![deny(warnings)]
#![deny(clippy::large_stack_frames)]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is prohibited in embedded systems with HAL-managed resources"
)]

pub use digivice_bsp as board;
pub use digivice_core::domain;
pub use digivice_core::gesture;
pub use digivice_core::pet;
pub use digivice_core::storage;

pub mod app;
pub mod ipc;
pub mod radio;
pub mod tasks;
pub mod ui;
