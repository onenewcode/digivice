//! Single-flight persistence transport: producers must wait for completion before signaling again.

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};

use crate::storage::{SaveCompletion, SaveRequest};

pub type SaveRequestSignal = Signal<CriticalSectionRawMutex, SaveRequest>;
pub type SaveCompletionSignal = Signal<CriticalSectionRawMutex, SaveCompletion>;
