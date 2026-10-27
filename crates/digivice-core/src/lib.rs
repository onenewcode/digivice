#![no_std]
#![forbid(unsafe_code)]
#![deny(warnings)]
#![deny(clippy::large_stack_frames)]
#![cfg_attr(not(target_arch = "riscv32"), allow(clippy::large_stack_frames))]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is prohibited in embedded systems with HAL-managed resources"
)]

pub mod domain;
pub mod drivers;
pub mod gesture;
pub mod pet;
pub mod protocol;
pub mod storage;
pub mod ui;

// Convenience aliases
pub use drivers::imu;
pub use drivers::rtc;
pub use protocol as radio;
