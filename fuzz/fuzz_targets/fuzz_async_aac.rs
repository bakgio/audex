#![no_main]

#[path = "helpers.rs"]
mod helpers;

use audex::aac::AAC;
use audex::FileType;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > helpers::MAX_INPUT_SIZE {
        return;
    }
    let data_owned = data.to_vec();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let Some((_dir, path)) = helpers::write_temp_file_async(&data_owned, "aac").await else {
            return;
        };
        if let Ok(f) = AAC::load_async(&path).await {
            let _ = f.info();
            let _ = f.tags();
        }
    });
});
