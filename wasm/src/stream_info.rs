// Stream information wrapper exposed to JavaScript.
//
// Provides access to audio properties like duration, bitrate,
// sample rate, channels, and bit depth through getter methods
// that wasm-bindgen maps to JS property accessors.

use wasm_bindgen::prelude::*;

/// Audio stream properties extracted from the file header.
///
/// All fields are optional because not every format provides every
/// property (e.g. VBR files may not report a fixed bitrate).
#[wasm_bindgen]
pub struct WasmStreamInfo {
    length_secs: Option<f64>,
    bitrate: Option<u32>,
    sample_rate: Option<u32>,
    channels: Option<u16>,
    bits_per_sample: Option<u16>,
}

#[wasm_bindgen]
impl WasmStreamInfo {
    /// Duration of the audio in seconds, or `undefined` if unknown.
    #[wasm_bindgen(getter, js_name = "lengthSecs")]
    pub fn length_secs(&self) -> Option<f64> {
        self.length_secs
    }

    /// Bitrate in bits per second, or `undefined` if unknown.
    #[wasm_bindgen(getter)]
    pub fn bitrate(&self) -> Option<u32> {
        self.bitrate
    }

    /// Sample rate in Hz (e.g. 44100, 48000), or `undefined` if unknown.
    #[wasm_bindgen(getter, js_name = "sampleRate")]
    pub fn sample_rate(&self) -> Option<u32> {
        self.sample_rate
    }

    /// Number of audio channels (e.g. 2 for stereo), or `undefined` if unknown.
    #[wasm_bindgen(getter)]
    pub fn channels(&self) -> Option<u16> {
        self.channels
    }

    /// Bit depth (e.g. 16, 24), or `undefined` if unknown.
    #[wasm_bindgen(getter, js_name = "bitsPerSample")]
    pub fn bits_per_sample(&self) -> Option<u16> {
        self.bits_per_sample
    }
}

/// Build a `WasmStreamInfo` from audex's dynamic stream info.
pub fn from_dynamic(info: &audex::DynamicStreamInfo) -> WasmStreamInfo {
    use audex::StreamInfo;
    WasmStreamInfo {
        length_secs: info.length().map(|d| d.as_secs_f64()),
        bitrate: info.bitrate(),
        sample_rate: info.sample_rate(),
        channels: info.channels(),
        bits_per_sample: info.bits_per_sample(),
    }
}

pub fn empty() -> WasmStreamInfo {
    WasmStreamInfo {
        length_secs: None,
        bitrate: None,
        sample_rate: None,
        channels: None,
        bits_per_sample: None,
    }
}
