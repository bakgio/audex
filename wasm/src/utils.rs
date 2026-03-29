// Utility helpers for the WASM runtime environment.

/// Install the `console_error_panic_hook` so that Rust panics produce
/// readable stack traces in the browser console instead of opaque
/// "unreachable" errors.  Safe to call multiple times.
pub fn set_panic_hook() {
    console_error_panic_hook::set_once();
}
