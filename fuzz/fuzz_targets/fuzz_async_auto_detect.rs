//! Async fuzz target for automatic format detection.
//!
//! Exercises `File::load_from_buffer_async` and the async loading path for
//! every format parser in the crate.

#![no_main]

#[path = "helpers.rs"]
mod helpers;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > helpers::MAX_INPUT_SIZE {
        return;
    }
    let data = data.to_vec();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        if let Ok(mut file) = audex::File::load_from_buffer_async(data.clone(), None).await {
            helpers::exercise_dynamic_file_async(&mut file, Some(&data)).await;
        }
    });
});
