#![no_std]
#![forbid(unsafe_code)]
#![deny(warnings)]
#![deny(clippy::large_stack_frames)]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is prohibited in embedded systems with HAL-managed resources"
)]

pub mod app;
pub mod board;
pub mod domain;
pub mod gesture;
pub mod ipc;
pub mod pet;
pub mod radio;
pub mod storage;
pub mod tasks;
pub mod ui;
