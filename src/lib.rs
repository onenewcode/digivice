#![no_std]
#![forbid(unsafe_code)]
#![deny(warnings)]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is prohibited in embedded systems with HAL-managed resources"
)]

pub mod app;
pub mod board;
pub mod gesture;
pub mod proto;
pub mod radio;
pub mod storage;
pub mod ui;
