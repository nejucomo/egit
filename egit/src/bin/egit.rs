//! Minimal binary entry point for the `egit` command-line application.
//!
//! All logic lives in the [`egit`] library crate; this binary simply delegates to [`egit::run`].

fn main() {
    egit::run();
}
