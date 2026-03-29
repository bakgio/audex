//! Demonstrates writing tags using format-specific types across all supported formats.
//!
//! Each audio format has its own tag system with a distinct API. This example
//! shows the set, remove, and clear pattern for every supported format in both
//! sync and async modes.
//!
//! For the unified `File` interface (which abstracts over all of these),
//! see the `file_operations` example instead.
//!
//! # Tag families
//!
//! - **EasyID3** (MP3) — simplified ID3v2 with human-readable keys
//! - **EasyMP4** (M4A/MP4) — MP4 atoms with custom key registration
//! - **VorbisComment** (FLAC, OggVorbis, OggOpus, OggFlac, OggSpeex, OggTheora)
//! - **APEv2** (MonkeysAudio, Musepack, WavPack, TrueAudio, TAK, OptimFROG)
//! - **ID3v2 raw** (AIFF, WAVE, DSF, DSDIFF) — raw frame IDs
//! - **ASF** (WMA/ASF)
//!
//! # Usage
//!
//! ```sh
//! # Sync only
//! cargo run --example write_tags -- <audio_file> sync
//!
//! # Async only (requires "async" feature)
//! cargo run --example write_tags --features async -- <audio_file> async
//!
//! # Both (default)
//! cargo run --example write_tags --features async -- <audio_file>
//! ```

#[allow(unused_imports)]
use audex::FileType;
use audex::Tags;
use audex::apev2::APEValue;
use std::error::Error;
use std::path::Path;

/// Copies the given audio file into a temporary directory so that destructive
/// operations (set, remove, clear) do not modify the user's original file.
/// Returns the path to the temporary copy. The caller must keep the returned
/// `TempDir` alive for the duration of the demo — dropping it deletes the copy.
fn copy_to_temp(original: &str) -> Result<(tempfile::TempDir, String), Box<dyn std::error::Error>> {
    let src = Path::new(original);
    let file_name = src.file_name().ok_or("input path has no file name")?;
    let tmp_dir = tempfile::tempdir()?;
    let dest = tmp_dir.path().join(file_name);
    std::fs::copy(src, &dest)?;
    let dest_str = dest.to_string_lossy().into_owned();
    Ok((tmp_dir, dest_str))
}

// EasyID3 — MP3

fn write_mp3(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::mp3::EasyMP3;

    let mut tagger = EasyMP3::load(path)?;
    if tagger.tags.is_none() {
        tagger.tags = Some(audex::easyid3::EasyID3::new());
    }
    if let Some(ref mut tags) = tagger.tags {
        tags.set("title", &[String::from("My Song")])?;
        tags.set("artist", &[String::from("Some Artist")])?;
    }
    tagger.save()?;

    let tagger = EasyMP3::load(path)?;
    if let Some(ref tags) = tagger.tags {
        println!("  title  = {:?}", tags.get("title"));
        println!("  artist = {:?}", tags.get("artist"));
    }

    let mut tagger = EasyMP3::load(path)?;
    if let Some(ref mut tags) = tagger.tags {
        tags.remove("title")?;
    }
    tagger.save()?;

    let mut tagger = EasyMP3::load(path)?;
    tagger.clear()?;
    Ok(())
}

#[cfg(feature = "async")]
async fn write_mp3_async(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::mp3::EasyMP3;

    let mut tagger = EasyMP3::load_async(path).await?;
    if tagger.tags.is_none() {
        tagger.tags = Some(audex::easyid3::EasyID3::new());
    }
    if let Some(ref mut tags) = tagger.tags {
        tags.set("title", &[String::from("My Song")])?;
        tags.set("artist", &[String::from("Some Artist")])?;
    }
    tagger.save_async().await?;

    let tagger = EasyMP3::load_async(path).await?;
    if let Some(ref tags) = tagger.tags {
        println!("  title  = {:?}", tags.get("title"));
        println!("  artist = {:?}", tags.get("artist"));
    }

    let mut tagger = EasyMP3::load_async(path).await?;
    if let Some(ref mut tags) = tagger.tags {
        tags.remove("title")?;
    }
    tagger.save_async().await?;

    let mut tagger = EasyMP3::load_async(path).await?;
    tagger.clear_async().await?;
    Ok(())
}

// EasyMP4 — M4A/MP4/M4B

fn write_mp4(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::easymp4::EasyMP4;

    let mut tagger = EasyMP4::load(path)?;
    tagger.register_text_key("my_tag", "----:TXXX:My Tag")?;
    tagger.set("title", vec![String::from("My Song")])?;
    tagger.set("my_tag", vec![String::from("custom_value")])?;
    tagger.save()?;

    let mut tagger = EasyMP4::load(path)?;
    tagger.register_text_key("my_tag", "----:TXXX:My Tag")?;
    println!("  title  = {:?}", tagger.get("title"));
    println!("  my_tag = {:?}", tagger.get("my_tag"));

    let mut tagger = EasyMP4::load(path)?;
    tagger.register_text_key("my_tag", "----:TXXX:My Tag")?;
    tagger.remove("my_tag")?;
    tagger.save()?;

    let mut tagger = EasyMP4::load(path)?;
    tagger.clear()?;
    Ok(())
}

#[cfg(feature = "async")]
async fn write_mp4_async(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::easymp4::EasyMP4;

    let mut tagger = EasyMP4::load_async(path).await?;
    tagger.register_text_key("my_tag", "----:TXXX:My Tag")?;
    tagger.set("title", vec![String::from("My Song")])?;
    tagger.set("my_tag", vec![String::from("custom_value")])?;
    tagger.save_async().await?;

    let mut tagger = EasyMP4::load_async(path).await?;
    tagger.register_text_key("my_tag", "----:TXXX:My Tag")?;
    println!("  title  = {:?}", tagger.get("title"));
    println!("  my_tag = {:?}", tagger.get("my_tag"));

    let mut tagger = EasyMP4::load_async(path).await?;
    tagger.register_text_key("my_tag", "----:TXXX:My Tag")?;
    tagger.remove("my_tag")?;
    tagger.save_async().await?;

    let mut tagger = EasyMP4::load_async(path).await?;
    tagger.clear_async().await?;
    Ok(())
}

// VorbisComment — FLAC

fn write_flac(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::flac::FLAC;

    let mut tagger = FLAC::load(path)?;
    if tagger.tags.is_none() {
        tagger.add_tags()?;
    }
    tagger.set("title", vec!["My Song".to_string()])?;
    tagger.set("artist", vec!["Some Artist".to_string()])?;
    tagger.save()?;

    let tagger = FLAC::load(path)?;
    println!("  title  = {:?}", tagger.get("title"));
    println!("  artist = {:?}", tagger.get("artist"));

    let mut tagger = FLAC::load(path)?;
    tagger.remove("title")?;
    tagger.save()?;

    let mut tagger = FLAC::load(path)?;
    tagger.clear()?;
    Ok(())
}

#[cfg(feature = "async")]
async fn write_flac_async(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::flac::FLAC;

    let mut tagger = FLAC::load_async(path).await?;
    if tagger.tags.is_none() {
        tagger.add_tags()?;
    }
    tagger.set("title", vec!["My Song".to_string()])?;
    tagger.set("artist", vec!["Some Artist".to_string()])?;
    tagger.save_async().await?;

    let tagger = FLAC::load_async(path).await?;
    println!("  title  = {:?}", tagger.get("title"));
    println!("  artist = {:?}", tagger.get("artist"));

    let mut tagger = FLAC::load_async(path).await?;
    tagger.remove("title")?;
    tagger.save_async().await?;

    let mut tagger = FLAC::load_async(path).await?;
    tagger.clear_async().await?;
    Ok(())
}

// VorbisComment — OggVorbis (tags wrapped in Option<VorbisComment>)

fn write_ogg_vorbis(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::oggvorbis::OggVorbis;

    let mut tagger = OggVorbis::load(path)?;
    if tagger.tags.is_none() {
        tagger.add_tags()?;
    }
    if let Some(ref mut tags) = tagger.tags {
        tags.set("title", vec!["My Song".to_string()]);
    }
    if let Some(ref mut tags) = tagger.tags {
        tags.set("artist", vec!["Some Artist".to_string()]);
    }
    tagger.save()?;

    let tagger = OggVorbis::load(path)?;
    println!("  title  = {:?}", tagger.get("title"));
    println!("  artist = {:?}", tagger.get("artist"));

    let mut tagger = OggVorbis::load(path)?;
    tagger.remove("title")?;
    tagger.save()?;

    let mut tagger = OggVorbis::load(path)?;
    tagger.clear()?;
    Ok(())
}

#[cfg(feature = "async")]
async fn write_ogg_vorbis_async(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::oggvorbis::OggVorbis;

    let mut tagger = OggVorbis::load_async(path).await?;
    if tagger.tags.is_none() {
        tagger.add_tags()?;
    }
    if let Some(ref mut tags) = tagger.tags {
        tags.set("title", vec!["My Song".to_string()]);
    }
    if let Some(ref mut tags) = tagger.tags {
        tags.set("artist", vec!["Some Artist".to_string()]);
    }
    tagger.save_async().await?;

    let tagger = OggVorbis::load_async(path).await?;
    println!("  title  = {:?}", tagger.get("title"));
    println!("  artist = {:?}", tagger.get("artist"));

    let mut tagger = OggVorbis::load_async(path).await?;
    tagger.remove("title")?;
    tagger.save_async().await?;

    let mut tagger = OggVorbis::load_async(path).await?;
    tagger.clear_async().await?;
    Ok(())
}

// VorbisComment — OggOpus

fn write_ogg_opus(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::oggopus::OggOpus;

    let mut tagger = OggOpus::load(path)?;
    tagger.set("title", vec!["My Song".to_string()])?;
    tagger.set("artist", vec!["Some Artist".to_string()])?;
    tagger.save()?;

    let tagger = OggOpus::load(path)?;
    println!("  title  = {:?}", tagger.get("title"));
    println!("  artist = {:?}", tagger.get("artist"));

    let mut tagger = OggOpus::load(path)?;
    tagger.remove("title")?;
    tagger.save()?;

    let mut tagger = OggOpus::load(path)?;
    tagger.clear()?;
    Ok(())
}

#[cfg(feature = "async")]
async fn write_ogg_opus_async(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::oggopus::OggOpus;

    let mut tagger = OggOpus::load_async(path).await?;
    tagger.set("title", vec!["My Song".to_string()])?;
    tagger.set("artist", vec!["Some Artist".to_string()])?;
    tagger.save_async().await?;

    let tagger = OggOpus::load_async(path).await?;
    println!("  title  = {:?}", tagger.get("title"));
    println!("  artist = {:?}", tagger.get("artist"));

    let mut tagger = OggOpus::load_async(path).await?;
    tagger.remove("title")?;
    tagger.save_async().await?;

    let mut tagger = OggOpus::load_async(path).await?;
    tagger.clear_async().await?;
    Ok(())
}

// VorbisComment — OggFlac

fn write_ogg_flac(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::oggflac::OggFlac;

    let mut tagger = OggFlac::load(path)?;
    tagger.set("title", vec!["My Song".to_string()])?;
    tagger.set("artist", vec!["Some Artist".to_string()])?;
    tagger.save()?;

    let tagger = OggFlac::load(path)?;
    println!("  title  = {:?}", tagger.get("title"));
    println!("  artist = {:?}", tagger.get("artist"));

    let mut tagger = OggFlac::load(path)?;
    tagger.remove("title")?;
    tagger.save()?;

    let mut tagger = OggFlac::load(path)?;
    tagger.clear()?;
    Ok(())
}

#[cfg(feature = "async")]
async fn write_ogg_flac_async(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::oggflac::OggFlac;

    let mut tagger = OggFlac::load_async(path).await?;
    tagger.set("title", vec!["My Song".to_string()])?;
    tagger.set("artist", vec!["Some Artist".to_string()])?;
    tagger.save_async().await?;

    let tagger = OggFlac::load_async(path).await?;
    println!("  title  = {:?}", tagger.get("title"));
    println!("  artist = {:?}", tagger.get("artist"));

    let mut tagger = OggFlac::load_async(path).await?;
    tagger.remove("title")?;
    tagger.save_async().await?;

    let mut tagger = OggFlac::load_async(path).await?;
    tagger.clear_async().await?;
    Ok(())
}

// VorbisComment — OggSpeex

fn write_ogg_speex(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::oggspeex::OggSpeex;

    let mut tagger = OggSpeex::load(path)?;
    tagger.set("title", vec!["My Song".to_string()])?;
    tagger.set("artist", vec!["Some Artist".to_string()])?;
    tagger.save()?;

    let tagger = OggSpeex::load(path)?;
    println!("  title  = {:?}", tagger.get("title"));
    println!("  artist = {:?}", tagger.get("artist"));

    let mut tagger = OggSpeex::load(path)?;
    tagger.remove("title")?;
    tagger.save()?;

    let mut tagger = OggSpeex::load(path)?;
    tagger.clear()?;
    Ok(())
}

#[cfg(feature = "async")]
async fn write_ogg_speex_async(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::oggspeex::OggSpeex;

    let mut tagger = OggSpeex::load_async(path).await?;
    tagger.set("title", vec!["My Song".to_string()])?;
    tagger.set("artist", vec!["Some Artist".to_string()])?;
    tagger.save_async().await?;

    let tagger = OggSpeex::load_async(path).await?;
    println!("  title  = {:?}", tagger.get("title"));
    println!("  artist = {:?}", tagger.get("artist"));

    let mut tagger = OggSpeex::load_async(path).await?;
    tagger.remove("title")?;
    tagger.save_async().await?;

    let mut tagger = OggSpeex::load_async(path).await?;
    tagger.clear_async().await?;
    Ok(())
}

// VorbisComment — OggTheora

fn write_ogg_theora(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::oggtheora::OggTheora;

    let mut tagger = OggTheora::load(path)?;
    tagger.set("title", vec!["My Song".to_string()])?;
    tagger.set("artist", vec!["Some Artist".to_string()])?;
    tagger.save()?;

    let tagger = OggTheora::load(path)?;
    println!("  title  = {:?}", tagger.get("title"));
    println!("  artist = {:?}", tagger.get("artist"));

    let mut tagger = OggTheora::load(path)?;
    tagger.remove("title")?;
    tagger.save()?;

    let mut tagger = OggTheora::load(path)?;
    tagger.clear()?;
    Ok(())
}

#[cfg(feature = "async")]
async fn write_ogg_theora_async(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::oggtheora::OggTheora;

    let mut tagger = OggTheora::load_async(path).await?;
    tagger.set("title", vec!["My Song".to_string()])?;
    tagger.set("artist", vec!["Some Artist".to_string()])?;
    tagger.save_async().await?;

    let tagger = OggTheora::load_async(path).await?;
    println!("  title  = {:?}", tagger.get("title"));
    println!("  artist = {:?}", tagger.get("artist"));

    let mut tagger = OggTheora::load_async(path).await?;
    tagger.remove("title")?;
    tagger.save_async().await?;

    let mut tagger = OggTheora::load_async(path).await?;
    tagger.clear_async().await?;
    Ok(())
}

// ASF — WMA/ASF
// Note: get() returns a Vec of ASFAttribute (not Option), check .is_empty()

fn write_asf(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::asf::ASF;

    let mut tagger = ASF::load(path)?;
    tagger.set("Title", vec!["My Song".to_string()]);
    tagger.set("Author", vec!["Some Artist".to_string()]);
    tagger.save()?;

    let tagger = ASF::load(path)?;
    println!("  Title  = {:?}", tagger.get("Title"));
    println!("  Author = {:?}", tagger.get("Author"));

    let mut tagger = ASF::load(path)?;
    tagger.remove("Title")?;
    tagger.save()?;

    let mut tagger = ASF::load(path)?;
    tagger.clear()?;
    Ok(())
}

#[cfg(feature = "async")]
async fn write_asf_async(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::asf::ASF;

    let mut tagger = ASF::load_async(path).await?;
    tagger.set("Title", vec!["My Song".to_string()]);
    tagger.set("Author", vec!["Some Artist".to_string()]);
    tagger.save_async().await?;

    let tagger = ASF::load_async(path).await?;
    println!("  Title  = {:?}", tagger.get("Title"));
    println!("  Author = {:?}", tagger.get("Author"));

    let mut tagger = ASF::load_async(path).await?;
    tagger.remove("Title")?;
    tagger.save_async().await?;

    let mut tagger = ASF::load_async(path).await?;
    tagger.clear_async().await?;
    Ok(())
}

// APEv2 — MonkeysAudio (.ape)
// Tags use APEValue::text() and are wrapped in Option<APEv2Tags>

fn write_monkeysaudio(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::monkeysaudio::MonkeysAudio;

    let mut tagger = MonkeysAudio::load(path)?;
    if tagger.tags.is_none() {
        tagger.add_tags()?;
    }
    if let Some(ref mut tags) = tagger.tags {
        tags.set("Title", APEValue::text("My Song"))?;
        tags.set("Artist", APEValue::text("Some Artist"))?;
    }
    tagger.save()?;

    let tagger = MonkeysAudio::load(path)?;
    if let Some(ref tags) = tagger.tags {
        println!("  Title  = {:?}", tags.get("Title"));
        println!("  Artist = {:?}", tags.get("Artist"));
    }

    let mut tagger = MonkeysAudio::load(path)?;
    if let Some(ref mut tags) = tagger.tags {
        let _ = tags.remove("Title");
    }
    tagger.save()?;

    let mut tagger = MonkeysAudio::load(path)?;
    tagger.clear()?;
    Ok(())
}

#[cfg(feature = "async")]
async fn write_monkeysaudio_async(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::monkeysaudio::MonkeysAudio;

    let mut tagger = MonkeysAudio::load_async(path).await?;
    if tagger.tags.is_none() {
        tagger.add_tags()?;
    }
    if let Some(ref mut tags) = tagger.tags {
        tags.set("Title", APEValue::text("My Song"))?;
        tags.set("Artist", APEValue::text("Some Artist"))?;
    }
    tagger.save_async().await?;

    let tagger = MonkeysAudio::load_async(path).await?;
    if let Some(ref tags) = tagger.tags {
        println!("  Title  = {:?}", tags.get("Title"));
        println!("  Artist = {:?}", tags.get("Artist"));
    }

    let mut tagger = MonkeysAudio::load_async(path).await?;
    if let Some(ref mut tags) = tagger.tags {
        let _ = tags.remove("Title");
    }
    tagger.save_async().await?;

    let mut tagger = MonkeysAudio::load_async(path).await?;
    tagger.clear_async().await?;
    Ok(())
}

// APEv2 — Musepack (.mpc)

fn write_musepack(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::musepack::Musepack;

    let mut tagger = Musepack::load(path)?;
    if tagger.tags.is_none() {
        tagger.add_tags()?;
    }
    if let Some(ref mut tags) = tagger.tags {
        tags.set("Title", APEValue::text("My Song"))?;
        tags.set("Artist", APEValue::text("Some Artist"))?;
    }
    tagger.save()?;

    let tagger = Musepack::load(path)?;
    if let Some(ref tags) = tagger.tags {
        println!("  Title  = {:?}", tags.get("Title"));
        println!("  Artist = {:?}", tags.get("Artist"));
    }

    let mut tagger = Musepack::load(path)?;
    if let Some(ref mut tags) = tagger.tags {
        let _ = tags.remove("Title");
    }
    tagger.save()?;

    let mut tagger = Musepack::load(path)?;
    tagger.clear()?;
    Ok(())
}

#[cfg(feature = "async")]
async fn write_musepack_async(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::musepack::Musepack;

    let mut tagger = Musepack::load_async(path).await?;
    if tagger.tags.is_none() {
        tagger.add_tags()?;
    }
    if let Some(ref mut tags) = tagger.tags {
        tags.set("Title", APEValue::text("My Song"))?;
        tags.set("Artist", APEValue::text("Some Artist"))?;
    }
    tagger.save_async().await?;

    let tagger = Musepack::load_async(path).await?;
    if let Some(ref tags) = tagger.tags {
        println!("  Title  = {:?}", tags.get("Title"));
        println!("  Artist = {:?}", tags.get("Artist"));
    }

    let mut tagger = Musepack::load_async(path).await?;
    if let Some(ref mut tags) = tagger.tags {
        let _ = tags.remove("Title");
    }
    tagger.save_async().await?;

    let mut tagger = Musepack::load_async(path).await?;
    tagger.clear_async().await?;
    Ok(())
}

// APEv2 — WavPack (.wv)

fn write_wavpack(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::wavpack::WavPack;

    let mut tagger = WavPack::load(path)?;
    if tagger.tags.is_none() {
        tagger.add_tags()?;
    }
    if let Some(ref mut tags) = tagger.tags {
        tags.set("Title", APEValue::text("My Song"))?;
        tags.set("Artist", APEValue::text("Some Artist"))?;
    }
    tagger.save()?;

    let tagger = WavPack::load(path)?;
    if let Some(ref tags) = tagger.tags {
        println!("  Title  = {:?}", tags.get("Title"));
        println!("  Artist = {:?}", tags.get("Artist"));
    }

    let mut tagger = WavPack::load(path)?;
    if let Some(ref mut tags) = tagger.tags {
        let _ = tags.remove("Title");
    }
    tagger.save()?;

    let mut tagger = WavPack::load(path)?;
    tagger.clear()?;
    Ok(())
}

#[cfg(feature = "async")]
async fn write_wavpack_async(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::wavpack::WavPack;

    let mut tagger = WavPack::load_async(path).await?;
    if tagger.tags.is_none() {
        tagger.add_tags()?;
    }
    if let Some(ref mut tags) = tagger.tags {
        tags.set("Title", APEValue::text("My Song"))?;
        tags.set("Artist", APEValue::text("Some Artist"))?;
    }
    tagger.save_async().await?;

    let tagger = WavPack::load_async(path).await?;
    if let Some(ref tags) = tagger.tags {
        println!("  Title  = {:?}", tags.get("Title"));
        println!("  Artist = {:?}", tags.get("Artist"));
    }

    let mut tagger = WavPack::load_async(path).await?;
    if let Some(ref mut tags) = tagger.tags {
        let _ = tags.remove("Title");
    }
    tagger.save_async().await?;

    let mut tagger = WavPack::load_async(path).await?;
    tagger.clear_async().await?;
    Ok(())
}

// APEv2 — TrueAudio (.tta)

fn write_trueaudio(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::trueaudio::TrueAudio;

    let mut tagger = TrueAudio::load(path)?;
    if tagger.tags.is_none() {
        tagger.assign_ape_tags();
    }
    if let Some(ref mut tags) = tagger.tags {
        tags.set("Title", APEValue::text("My Song"))?;
        tags.set("Artist", APEValue::text("Some Artist"))?;
    }
    tagger.save()?;

    let tagger = TrueAudio::load(path)?;
    if let Some(ref tags) = tagger.tags {
        println!("  Title  = {:?}", tags.get("Title"));
        println!("  Artist = {:?}", tags.get("Artist"));
    }

    let mut tagger = TrueAudio::load(path)?;
    if let Some(ref mut tags) = tagger.tags {
        let _ = tags.remove("Title");
    }
    tagger.save()?;

    let mut tagger = TrueAudio::load(path)?;
    tagger.clear()?;
    Ok(())
}

#[cfg(feature = "async")]
async fn write_trueaudio_async(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::trueaudio::TrueAudio;

    let mut tagger = TrueAudio::load_async(path).await?;
    if tagger.tags.is_none() {
        tagger.assign_ape_tags();
    }
    if let Some(ref mut tags) = tagger.tags {
        tags.set("Title", APEValue::text("My Song"))?;
        tags.set("Artist", APEValue::text("Some Artist"))?;
    }
    tagger.save_async().await?;

    let tagger = TrueAudio::load_async(path).await?;
    if let Some(ref tags) = tagger.tags {
        println!("  Title  = {:?}", tags.get("Title"));
        println!("  Artist = {:?}", tags.get("Artist"));
    }

    let mut tagger = TrueAudio::load_async(path).await?;
    if let Some(ref mut tags) = tagger.tags {
        let _ = tags.remove("Title");
    }
    tagger.save_async().await?;

    let mut tagger = TrueAudio::load_async(path).await?;
    tagger.clear_async().await?;
    Ok(())
}

// APEv2 — TAK (.tak)

fn write_tak(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::tak::TAK;

    let mut tagger = TAK::load(path)?;
    if tagger.tags.is_none() {
        tagger.add_tags()?;
    }
    if let Some(ref mut tags) = tagger.tags {
        tags.set("Title", APEValue::text("My Song"))?;
        tags.set("Artist", APEValue::text("Some Artist"))?;
    }
    tagger.save()?;

    let tagger = TAK::load(path)?;
    if let Some(ref tags) = tagger.tags {
        println!("  Title  = {:?}", tags.get("Title"));
        println!("  Artist = {:?}", tags.get("Artist"));
    }

    let mut tagger = TAK::load(path)?;
    if let Some(ref mut tags) = tagger.tags {
        let _ = tags.remove("Title");
    }
    tagger.save()?;

    let mut tagger = TAK::load(path)?;
    tagger.clear()?;
    Ok(())
}

#[cfg(feature = "async")]
async fn write_tak_async(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::tak::TAK;

    let mut tagger = TAK::load_async(path).await?;
    if tagger.tags.is_none() {
        tagger.add_tags()?;
    }
    if let Some(ref mut tags) = tagger.tags {
        tags.set("Title", APEValue::text("My Song"))?;
        tags.set("Artist", APEValue::text("Some Artist"))?;
    }
    tagger.save_async().await?;

    let tagger = TAK::load_async(path).await?;
    if let Some(ref tags) = tagger.tags {
        println!("  Title  = {:?}", tags.get("Title"));
        println!("  Artist = {:?}", tags.get("Artist"));
    }

    let mut tagger = TAK::load_async(path).await?;
    if let Some(ref mut tags) = tagger.tags {
        let _ = tags.remove("Title");
    }
    tagger.save_async().await?;

    let mut tagger = TAK::load_async(path).await?;
    tagger.clear_async().await?;
    Ok(())
}

// APEv2 — OptimFROG (.ofr)

fn write_optimfrog(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::optimfrog::OptimFROG;

    let mut tagger = OptimFROG::load(path)?;
    if tagger.tags.is_none() {
        tagger.add_tags()?;
    }
    if let Some(ref mut tags) = tagger.tags {
        tags.set("Title", APEValue::text("My Song"))?;
        tags.set("Artist", APEValue::text("Some Artist"))?;
    }
    tagger.save()?;

    let tagger = OptimFROG::load(path)?;
    if let Some(ref tags) = tagger.tags {
        println!("  Title  = {:?}", tags.get("Title"));
        println!("  Artist = {:?}", tags.get("Artist"));
    }

    let mut tagger = OptimFROG::load(path)?;
    if let Some(ref mut tags) = tagger.tags {
        let _ = tags.remove("Title");
    }
    tagger.save()?;

    let mut tagger = OptimFROG::load(path)?;
    tagger.clear()?;
    Ok(())
}

#[cfg(feature = "async")]
async fn write_optimfrog_async(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::optimfrog::OptimFROG;

    let mut tagger = OptimFROG::load_async(path).await?;
    if tagger.tags.is_none() {
        tagger.add_tags()?;
    }
    if let Some(ref mut tags) = tagger.tags {
        tags.set("Title", APEValue::text("My Song"))?;
        tags.set("Artist", APEValue::text("Some Artist"))?;
    }
    tagger.save_async().await?;

    let tagger = OptimFROG::load_async(path).await?;
    if let Some(ref tags) = tagger.tags {
        println!("  Title  = {:?}", tags.get("Title"));
        println!("  Artist = {:?}", tags.get("Artist"));
    }

    let mut tagger = OptimFROG::load_async(path).await?;
    if let Some(ref mut tags) = tagger.tags {
        let _ = tags.remove("Title");
    }
    tagger.save_async().await?;

    let mut tagger = OptimFROG::load_async(path).await?;
    tagger.clear_async().await?;
    Ok(())
}

// ID3v2 raw — AIFF (.aiff)
// Tags use raw 4-char frame IDs (TIT2, TPE1, etc.)

fn write_aiff(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::aiff::AIFF;

    let mut tagger = AIFF::load(path)?;
    if tagger.tags.is_none() {
        tagger.add_tags()?;
    }
    if let Some(ref mut tags) = tagger.tags {
        tags.set("TIT2", vec!["My Song".to_string()]);
        tags.set("TPE1", vec!["Some Artist".to_string()]);
    }
    tagger.save()?;

    let tagger = AIFF::load(path)?;
    if let Some(ref tags) = tagger.tags {
        println!("  TIT2 = {:?}", tags.get("TIT2"));
        println!("  TPE1 = {:?}", tags.get("TPE1"));
    }

    let mut tagger = AIFF::load(path)?;
    if let Some(ref mut tags) = tagger.tags {
        tags.remove("TIT2");
    }
    tagger.save()?;

    let mut tagger = AIFF::load(path)?;
    tagger.clear()?;
    Ok(())
}

#[cfg(feature = "async")]
async fn write_aiff_async(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::aiff::AIFF;

    let mut tagger = AIFF::load_async(path).await?;
    if tagger.tags.is_none() {
        tagger.add_tags()?;
    }
    if let Some(ref mut tags) = tagger.tags {
        tags.set("TIT2", vec!["My Song".to_string()]);
        tags.set("TPE1", vec!["Some Artist".to_string()]);
    }
    tagger.save_async().await?;

    let tagger = AIFF::load_async(path).await?;
    if let Some(ref tags) = tagger.tags {
        println!("  TIT2 = {:?}", tags.get("TIT2"));
        println!("  TPE1 = {:?}", tags.get("TPE1"));
    }

    let mut tagger = AIFF::load_async(path).await?;
    if let Some(ref mut tags) = tagger.tags {
        tags.remove("TIT2");
    }
    tagger.save_async().await?;

    let mut tagger = AIFF::load_async(path).await?;
    tagger.clear_async().await?;
    Ok(())
}

// ID3v2 raw — WAVE (.wav)

fn write_wave(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::wave::WAVE;

    let mut tagger = WAVE::load(path)?;
    if tagger.tags.is_none() {
        tagger.add_tags()?;
    }
    if let Some(ref mut tags) = tagger.tags {
        tags.set("TIT2", vec!["My Song".to_string()]);
        tags.set("TPE1", vec!["Some Artist".to_string()]);
    }
    tagger.save()?;

    let tagger = WAVE::load(path)?;
    if let Some(ref tags) = tagger.tags {
        println!("  TIT2 = {:?}", tags.get("TIT2"));
        println!("  TPE1 = {:?}", tags.get("TPE1"));
    }

    let mut tagger = WAVE::load(path)?;
    if let Some(ref mut tags) = tagger.tags {
        tags.remove("TIT2");
    }
    tagger.save()?;

    let mut tagger = WAVE::load(path)?;
    tagger.clear()?;
    Ok(())
}

#[cfg(feature = "async")]
async fn write_wave_async(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::wave::WAVE;

    let mut tagger = WAVE::load_async(path).await?;
    if tagger.tags.is_none() {
        tagger.add_tags()?;
    }
    if let Some(ref mut tags) = tagger.tags {
        tags.set("TIT2", vec!["My Song".to_string()]);
        tags.set("TPE1", vec!["Some Artist".to_string()]);
    }
    tagger.save_async().await?;

    let tagger = WAVE::load_async(path).await?;
    if let Some(ref tags) = tagger.tags {
        println!("  TIT2 = {:?}", tags.get("TIT2"));
        println!("  TPE1 = {:?}", tags.get("TPE1"));
    }

    let mut tagger = WAVE::load_async(path).await?;
    if let Some(ref mut tags) = tagger.tags {
        tags.remove("TIT2");
    }
    tagger.save_async().await?;

    let mut tagger = WAVE::load_async(path).await?;
    tagger.clear_async().await?;
    Ok(())
}

// ID3v2 raw — DSF (.dsf)

fn write_dsf(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::dsf::DSF;

    let mut tagger = DSF::load(path)?;
    if tagger.tags.is_none() {
        tagger.add_tags()?;
    }
    if let Some(ref mut tags) = tagger.tags {
        tags.set("TIT2", vec!["My Song".to_string()]);
        tags.set("TPE1", vec!["Some Artist".to_string()]);
    }
    tagger.save()?;

    let tagger = DSF::load(path)?;
    if let Some(ref tags) = tagger.tags {
        println!("  TIT2 = {:?}", tags.get("TIT2"));
        println!("  TPE1 = {:?}", tags.get("TPE1"));
    }

    let mut tagger = DSF::load(path)?;
    if let Some(ref mut tags) = tagger.tags {
        tags.remove("TIT2");
    }
    tagger.save()?;

    let mut tagger = DSF::load(path)?;
    tagger.clear()?;
    Ok(())
}

#[cfg(feature = "async")]
async fn write_dsf_async(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::dsf::DSF;

    let mut tagger = DSF::load_async(path).await?;
    if tagger.tags.is_none() {
        tagger.add_tags()?;
    }
    if let Some(ref mut tags) = tagger.tags {
        tags.set("TIT2", vec!["My Song".to_string()]);
        tags.set("TPE1", vec!["Some Artist".to_string()]);
    }
    tagger.save_async().await?;

    let tagger = DSF::load_async(path).await?;
    if let Some(ref tags) = tagger.tags {
        println!("  TIT2 = {:?}", tags.get("TIT2"));
        println!("  TPE1 = {:?}", tags.get("TPE1"));
    }

    let mut tagger = DSF::load_async(path).await?;
    if let Some(ref mut tags) = tagger.tags {
        tags.remove("TIT2");
    }
    tagger.save_async().await?;

    let mut tagger = DSF::load_async(path).await?;
    tagger.clear_async().await?;
    Ok(())
}

// ID3v2 raw — DSDIFF (.dff)

fn write_dsdiff(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::dsdiff::DSDIFF;

    let mut tagger = DSDIFF::load(path)?;
    if tagger.tags.is_none() {
        tagger.add_tags()?;
    }
    if let Some(ref mut tags) = tagger.tags {
        tags.set("TIT2", vec!["My Song".to_string()]);
        tags.set("TPE1", vec!["Some Artist".to_string()]);
    }
    tagger.save()?;

    let tagger = DSDIFF::load(path)?;
    if let Some(ref tags) = tagger.tags {
        println!("  TIT2 = {:?}", tags.get("TIT2"));
        println!("  TPE1 = {:?}", tags.get("TPE1"));
    }

    let mut tagger = DSDIFF::load(path)?;
    if let Some(ref mut tags) = tagger.tags {
        tags.remove("TIT2");
    }
    tagger.save()?;

    let mut tagger = DSDIFF::load(path)?;
    tagger.clear()?;
    Ok(())
}

#[cfg(feature = "async")]
async fn write_dsdiff_async(path: &str) -> Result<(), Box<dyn Error>> {
    use audex::dsdiff::DSDIFF;

    let mut tagger = DSDIFF::load_async(path).await?;
    if tagger.tags.is_none() {
        tagger.add_tags()?;
    }
    if let Some(ref mut tags) = tagger.tags {
        tags.set("TIT2", vec!["My Song".to_string()]);
        tags.set("TPE1", vec!["Some Artist".to_string()]);
    }
    tagger.save_async().await?;

    let tagger = DSDIFF::load_async(path).await?;
    if let Some(ref tags) = tagger.tags {
        println!("  TIT2 = {:?}", tags.get("TIT2"));
        println!("  TPE1 = {:?}", tags.get("TPE1"));
    }

    let mut tagger = DSDIFF::load_async(path).await?;
    if let Some(ref mut tags) = tagger.tags {
        tags.remove("TIT2");
    }
    tagger.save_async().await?;

    let mut tagger = DSDIFF::load_async(path).await?;
    tagger.clear_async().await?;
    Ok(())
}

// Main — dispatch by extension, run sync and/or async

fn dispatch_sync(file_path: &str, ext: &str) -> Result<(), Box<dyn Error>> {
    println!("=== Sync: {} ===", ext.to_uppercase());
    match ext {
        "mp3" => write_mp3(file_path)?,
        "m4a" | "mp4" | "m4b" => write_mp4(file_path)?,
        "flac" => write_flac(file_path)?,
        "ogg" => write_ogg_vorbis(file_path)?,
        "opus" => write_ogg_opus(file_path)?,
        "oga" => write_ogg_flac(file_path)?,
        "spx" => write_ogg_speex(file_path)?,
        "ogv" => write_ogg_theora(file_path)?,
        "asf" | "wma" => write_asf(file_path)?,
        "ape" => write_monkeysaudio(file_path)?,
        "mpc" => write_musepack(file_path)?,
        "wv" => write_wavpack(file_path)?,
        "tta" => write_trueaudio(file_path)?,
        "tak" => write_tak(file_path)?,
        "ofr" => write_optimfrog(file_path)?,
        "aiff" => write_aiff(file_path)?,
        "wav" => write_wave(file_path)?,
        "dsf" => write_dsf(file_path)?,
        "dff" => write_dsdiff(file_path)?,
        _ => {
            eprintln!("Unsupported format: .{}", ext);
            std::process::exit(1);
        }
    }
    Ok(())
}

#[cfg(feature = "async")]
async fn dispatch_async(file_path: &str, ext: &str) -> Result<(), Box<dyn Error>> {
    println!("\n=== Async: {} ===", ext.to_uppercase());
    match ext {
        "mp3" => write_mp3_async(file_path).await?,
        "m4a" | "mp4" | "m4b" => write_mp4_async(file_path).await?,
        "flac" => write_flac_async(file_path).await?,
        "ogg" => write_ogg_vorbis_async(file_path).await?,
        "opus" => write_ogg_opus_async(file_path).await?,
        "oga" => write_ogg_flac_async(file_path).await?,
        "spx" => write_ogg_speex_async(file_path).await?,
        "ogv" => write_ogg_theora_async(file_path).await?,
        "asf" | "wma" => write_asf_async(file_path).await?,
        "ape" => write_monkeysaudio_async(file_path).await?,
        "mpc" => write_musepack_async(file_path).await?,
        "wv" => write_wavpack_async(file_path).await?,
        "tta" => write_trueaudio_async(file_path).await?,
        "tak" => write_tak_async(file_path).await?,
        "ofr" => write_optimfrog_async(file_path).await?,
        "aiff" => write_aiff_async(file_path).await?,
        "wav" => write_wave_async(file_path).await?,
        "dsf" => write_dsf_async(file_path).await?,
        "dff" => write_dsdiff_async(file_path).await?,
        _ => {
            eprintln!("Unsupported format: .{}", ext);
            std::process::exit(1);
        }
    }
    Ok(())
}

fn parse_args() -> (String, String) {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <audio_file> [mode]", args[0]);
        eprintln!();
        eprintln!("Modes:");
        eprintln!("  sync  - Run synchronous operations only");
        eprintln!("  async - Run asynchronous operations only");
        eprintln!("  both  - Run both (default)");
        eprintln!();
        eprintln!("Supported formats:");
        eprintln!("  mp3, m4a, flac, ogg, opus, oga (treats as Ogg FLAC),");
        eprintln!("  spx, ogv, asf, wma, ape, mpc, wv, tta, tak, ofr,");
        eprintln!("  aiff, wav, dsf, dff");
        std::process::exit(1);
    }

    let file_path = args[1].clone();
    let mode = if args.len() > 2 {
        args[2].to_lowercase()
    } else {
        "both".to_string()
    };

    if mode != "sync" && mode != "async" && mode != "both" {
        eprintln!(
            "Error: Invalid mode '{}'. Must be 'sync', 'async', or 'both'",
            mode
        );
        std::process::exit(1);
    }

    if !Path::new(&file_path).exists() {
        eprintln!("Error: File not found: {}", file_path);
        std::process::exit(1);
    }

    (file_path, mode)
}

fn get_ext(file_path: &str) -> String {
    Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
}

#[cfg(feature = "async")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let (file_path, mode) = parse_args();
    let ext = get_ext(&file_path);

    // WARNING: This example performs destructive tag operations (set, remove,
    // clear). To protect the original file we work on a temporary copy.
    let (_tmp_dir, tmp_path) = copy_to_temp(&file_path)?;
    println!("Working on temporary copy: {}", tmp_path);

    if mode == "sync" || mode == "both" {
        dispatch_sync(&tmp_path, &ext)?;
    }

    if mode == "async" || mode == "both" {
        dispatch_async(&tmp_path, &ext).await?;
    }

    println!("\nDone. Original file was not modified.");
    Ok(())
}

#[cfg(not(feature = "async"))]
fn main() -> Result<(), Box<dyn Error>> {
    let (file_path, _mode) = parse_args();
    let ext = get_ext(&file_path);

    // WARNING: This example performs destructive tag operations (set, remove,
    // clear). To protect the original file we work on a temporary copy.
    let (_tmp_dir, tmp_path) = copy_to_temp(&file_path)?;
    println!("Working on temporary copy: {}", tmp_path);

    dispatch_sync(&tmp_path, &ext)?;

    println!("\nDone. Original file was not modified.");
    Ok(())
}
