//! High-level CLI application library for `egit`.
//!
//! This crate exposes [`Options`] (a `clap`-derived struct for command-line argument parsing) and
//! [`run`], which parses those options and launches the `eframe` GUI application.
//!
//! The binary entry point lives in `src/bin/egit.rs` and simply calls [`run`].

use clap::Parser;
use eframe::egui;
use egit_repo_view::RepoView;
use std::path::PathBuf;

/// Command-line options for `egit`.
#[derive(Debug, Parser)]
#[command(
    name = "egit",
    about = "Interactive visual git history explorer",
    version
)]
pub struct Options {
    /// Path to the git repository to inspect.
    ///
    /// Defaults to the current working directory when not specified.
    #[arg(
        short = 'C',
        long = "repo",
        value_name = "PATH",
        default_value = "."
    )]
    pub repo: PathBuf,

    /// Window title override.
    #[arg(long, value_name = "TITLE")]
    pub title: Option<String>,
}

/// Parse command-line arguments and launch the egit GUI application.
///
/// This function does not return on success — it hands control to the `eframe` event loop.
/// On failure it prints an error message and exits with a non-zero status code.
pub fn run() {
    let opts = Options::parse();

    let title = opts
        .title
        .clone()
        .unwrap_or_else(|| format!("egit — {}", opts.repo.display()));

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(&title)
            .with_inner_size([1024.0, 768.0]),
        ..Default::default()
    };

    let repo_path = opts.repo.clone();

    eframe::run_native(
        &title,
        native_options,
        Box::new(move |_cc| {
            let view = RepoView::open(&repo_path).unwrap_or_else(|err| {
                eprintln!("egit: failed to open repository at '{}': {}", repo_path.display(), err);
                std::process::exit(1);
            });
            Ok(Box::new(EgitApp { view }))
        }),
    )
    .unwrap_or_else(|err| {
        eprintln!("egit: eframe error: {err}");
        std::process::exit(1);
    });
}

// ── App ──────────────────────────────────────────────────────────────────────

struct EgitApp {
    view: RepoView,
}

impl eframe::App for EgitApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.add(&mut self.view);
    }
}
