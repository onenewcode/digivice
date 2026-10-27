//! Runtime Flash owner. No other task can access the moved storage manager.

use crate::board::StorageManager;
use crate::ipc::{SAVE_COMPLETION_SIGNAL, SAVE_REQUEST_SIGNAL};
use esp_println::println;

#[embassy_executor::task]
pub async fn storage_task(mut storage: StorageManager<'static>) -> ! {
    loop {
        let request = SAVE_REQUEST_SIGNAL.wait().await;
        let completion = storage.execute_save(request);
        match completion.result {
            Ok(config) => println!("Save #{} committed seq={}", completion.id.get(), config.seq),
            Err(error) => println!("Save #{} failed: {}", completion.id.get(), error),
        }
        SAVE_COMPLETION_SIGNAL.signal(completion);
    }
}
