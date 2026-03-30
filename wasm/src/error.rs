// Error bridge between audex's Rust error types and JavaScript Error objects.
//
// Each AudexError variant is mapped to a named JS Error so that consumers
// can programmatically match on `error.name` (e.g. "UnsupportedFormat").

use std::panic::AssertUnwindSafe;

use wasm_bindgen::JsValue;

pub struct CaughtPanic<T> {
    pub result: Result<T, JsValue>,
    pub panicked: bool,
}

/// Convert an `AudexError` into a JavaScript `Error` with a descriptive
/// name and message, suitable for throwing in JS/TS consumers.
pub fn to_js_error(e: audex::AudexError) -> JsValue {
    let js_error = js_sys::Error::new(&e.to_string());
    js_error.set_name(&error_name(&e));
    js_error.into()
}

/// Catches panics from the inner closure and converts them to JS errors.
///
/// Prevents a panic in the core library from aborting the WASM runtime.
/// The closure is wrapped in `AssertUnwindSafe` so callers do not need
/// to satisfy `UnwindSafe` bounds on captured references.
///
/// SAFETY INVARIANT: closures passed to this function must not partially
/// mutate `&mut self` state before a potential panic point. All fallible
/// or panicking operations must complete before any assignment back to
/// `self` fields. For example, in `save()` the cursor clone is a
/// separate local — `self.original_bytes` is only overwritten after
/// `save_to_writer` succeeds (the `?` operator returns early on error).
/// If this invariant is violated, a caught panic would leave the
/// `AudioFile` in an inconsistent state visible to subsequent JS calls.
///
/// # Safety
///
/// Closures must not partially mutate `&mut self` state before a potential
/// panic point. If the closure panics after a partial write, the object
/// will be left in an inconsistent state that subsequent calls may observe.
///
/// All mutation methods on `AudioFile` should use `run_mutation_with_poison`
/// instead of calling `catch_panic` directly. The poison guard automatically
/// marks the instance as poisoned when a panic is caught, preventing further
/// use of a potentially corrupt object. Direct use of `catch_panic` bypasses
/// this protection and should be reserved for non-mutating or self-contained
/// operations where no `&mut self` state is at risk.
pub fn catch_panic<F, T>(f: F) -> Result<T, JsValue>
where
    F: FnOnce() -> Result<T, JsValue>,
{
    catch_panic_with_status(f).result
}

pub fn catch_panic_with_status<F, T>(f: F) -> CaughtPanic<T>
where
    F: FnOnce() -> Result<T, JsValue>,
{
    match std::panic::catch_unwind(AssertUnwindSafe(f)) {
        Ok(result) => CaughtPanic {
            result,
            panicked: false,
        },
        Err(panic_info) => {
            let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                format!("internal error: {}", s)
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                format!("internal error: {}", s)
            } else {
                "internal error: unexpected panic".to_string()
            };
            let error = js_sys::Error::new(&msg);
            CaughtPanic {
                result: Err(JsValue::from(error)),
                panicked: true,
            }
        }
    }
}

/// Map each error variant to a short, JS-friendly error name.
/// These names appear as `error.name` on the JavaScript side and
/// allow consumers to distinguish error categories without parsing messages.
fn error_name(e: &audex::AudexError) -> String {
    match e {
        audex::AudexError::Io(_) => "IoError",
        audex::AudexError::UnsupportedFormat(_) => "UnsupportedFormat",
        audex::AudexError::InvalidData(_) => "InvalidData",
        audex::AudexError::ParseError(_) => "ParseError",
        audex::AudexError::HeaderNotFound => "HeaderNotFound",
        audex::AudexError::FormatError(_) => "FormatError",
        audex::AudexError::Unsupported(_) => "Unsupported",
        audex::AudexError::NotImplemented(_) => "NotImplemented",
        audex::AudexError::InvalidOperation(_) => "InvalidOperation",
        audex::AudexError::ASF(_) => "ASFError",
        audex::AudexError::FLACNoHeader => "FLACNoHeader",
        audex::AudexError::FLACVorbis => "FLACVorbis",
        audex::AudexError::APENoHeader => "APENoHeader",
        audex::AudexError::APEBadItem(_) => "APEBadItem",
        audex::AudexError::APEUnsupportedVersion => "APEUnsupportedVersion",
        audex::AudexError::WAVError(_) => "WAVError",
        audex::AudexError::WAVInvalidChunk(_) => "WAVInvalidChunk",
        audex::AudexError::AACError(_) => "AACError",
        audex::AudexError::AC3Error(_) => "AC3Error",
        audex::AudexError::AIFFError(_) => "AIFFError",
        audex::AudexError::IFFError(_) => "IFFError",
        audex::AudexError::MusepackHeaderError(_) => "MusepackError",
        audex::AudexError::TrueAudioHeaderError(_) => "TrueAudioError",
        audex::AudexError::TAKHeaderError(_) => "TAKError",
        audex::AudexError::WavPackHeaderError(_) => "WavPackError",
        audex::AudexError::OptimFROGHeaderError(_) => "OptimFROGError",
        audex::AudexError::MonkeysAudioHeaderError(_) => "MonkeysAudioError",
        audex::AudexError::DSFError(_) => "DSFError",
        audex::AudexError::NotImplementedMethod(_) => "NotImplementedMethod",
        audex::AudexError::TagOperationUnsupported(_) => "TagOperationUnsupported",
        audex::AudexError::ID3NoHeaderError => "ID3NoHeader",
        audex::AudexError::ID3BadUnsynchData => "ID3BadUnsynchData",
        audex::AudexError::ID3FrameTooShort { .. } => "ID3FrameTooShort",
        audex::AudexError::DepthLimitExceeded { .. } => "DepthLimitExceeded",
        audex::AudexError::InternalError(_) => "InternalError",
        _ => "UnknownError",
    }
    .to_string()
}
