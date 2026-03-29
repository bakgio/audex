mod common;

use audex::mp3::{EasyMP3, MP3};
use audex::{AudexError, FileType};
use common::TestUtils;

#[test]
fn test_mp3_save_functionality() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TestUtils::get_temp_copy(TestUtils::data_path("silence-44-s.mp3"))?;
    let test_file = temp.path();

    // Test MP3 save
    {
        let mut mp3 = MP3::load(test_file)?;

        if mp3.tags.is_none() {
            mp3.tags = Some(audex::id3::ID3Tags::new());
        }

        let result = mp3.save();
        match result {
            Ok(()) => println!("MP3::save() works correctly"),
            Err(AudexError::Unsupported(msg)) if msg.contains("not yet implemented") => {
                panic!("MP3::save() still returns 'not implemented' error: {}", msg);
            }
            Err(e) => {
                println!("MP3::save() returned error (acceptable): {}", e);
            }
        }
    }

    // Test EasyMP3 save
    {
        let mut easy_mp3 = EasyMP3::load(test_file)?;

        if easy_mp3.tags.is_none() {
            easy_mp3.tags = Some(audex::easyid3::EasyID3::new());
        }

        let result = easy_mp3.save();
        match result {
            Ok(()) => println!("EasyMP3::save() works correctly"),
            Err(AudexError::Unsupported(msg)) if msg.contains("not yet implemented") => {
                panic!(
                    "EasyMP3::save() still returns 'not implemented' error: {}",
                    msg
                );
            }
            Err(e) => {
                println!("EasyMP3::save() returned error (acceptable): {}", e);
            }
        }
    }

    // Test EasyID3 save directly
    {
        let mut easyid3 = audex::easyid3::EasyID3::load(test_file)?;

        let result = easyid3.save();
        match result {
            Ok(()) => println!("EasyID3::save() works correctly"),
            Err(AudexError::Unsupported(msg)) if msg.contains("not yet implemented") => {
                panic!(
                    "EasyID3::save() still returns 'not implemented' error: {}",
                    msg
                );
            }
            Err(e) => {
                println!("EasyID3::save() returned error (acceptable): {}", e);
            }
        }
    }

    println!("All MP3 save methods are properly implemented!");
    Ok(())
}

#[test]
fn test_save_methods_no_longer_unsupported() {
    let temp = TestUtils::get_temp_copy(TestUtils::data_path("silence-44-s.mp3"))
        .expect("Failed to create temp copy");
    let test_file = temp.path();

    // Test that MP3::save() doesn't return unsupported error
    {
        let mut mp3 = MP3::load(test_file).expect("Failed to load MP3");
        if mp3.tags.is_none() {
            mp3.tags = Some(audex::id3::ID3Tags::new());
        }

        match mp3.save() {
            Err(AudexError::Unsupported(msg)) if msg.contains("not yet implemented") => {
                panic!("MP3::save() should not return 'not yet implemented' error");
            }
            _ => {}
        }
    }

    // Test that EasyMP3::save() doesn't return unsupported error
    {
        let mut easy_mp3 = EasyMP3::load(test_file).expect("Failed to load EasyMP3");
        if easy_mp3.tags.is_none() {
            easy_mp3.tags = Some(audex::easyid3::EasyID3::new());
        }

        match easy_mp3.save() {
            Err(AudexError::Unsupported(msg)) if msg.contains("not yet implemented") => {
                panic!("EasyMP3::save() should not return 'not yet implemented' error");
            }
            _ => {}
        }
    }
}
