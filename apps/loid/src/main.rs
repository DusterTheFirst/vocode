#![forbid(unsafe_code)]

mod app;

use app::Application;
use audio::output::AudioSink;
use color_eyre::eyre::Context;
use tracing::*;
use util::install_tracing;

pub fn init() -> color_eyre::Result<Application> {
    #[cfg(not(target_arch = "wasm32"))]
    color_eyre::install().wrap_err("failed to install color_eyre")?;

    install_tracing().wrap_err("failed to install tracing_subscriber")?;

    trace!("Setting up audio");

    let audio_sink = AudioSink::new().wrap_err("failed to setup audio sink")?;

    info!("Starting Application");

    Ok(Application::new(audio_sink))
}

#[cfg(all(not(target_arch = "wasm32"), feature = "snmalloc"))]
#[global_allocator]
static ALLOC: snmalloc_rs::SnMalloc = snmalloc_rs::SnMalloc;

// When compiling natively:
#[cfg(not(target_arch = "wasm32"))]
fn main() -> color_eyre::Result<()> {
    eframe::run_native(
        "Fun with FFT",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Box::new(init().unwrap())),
    )
}

// ----------------------------------------------------------------------------
// When compiling for web:

/// This is the entry-point for all the web-assembly.
/// This is called once from the HTML.
/// It loads the app, installs some callbacks, then returns.
/// You can add more callbacks like this if you want to call in to your code.
#[cfg(target_arch = "wasm32")]
fn main() {
    std::panic::set_hook(Box::new(|panic_info| {
        // Try to show the panic in HTML
        web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.body())
            .and_then(|element| element.set_attribute("data-panicked", "true").ok());

        // Use console error panic hook to send the info to the console
        console_error_panic_hook::hook(panic_info);
    }));

    let app = match init() {
        Ok(app) => app,
        Err(err) => {
            error!("Encountered error in application initialization");

            // TODO: use console features to make this more integrated
            web_sys::console::error_1(
                &strip_ansi_escapes::strip_str(format!("Error: {err:?}")).into(),
            );

            return;
        }
    };

    match eframe::start_web("egui_canvas", Box::new(move |_cc| Box::new(app))) {
        Ok(()) => {
            info!("eframe successfully started");
        }
        Err(error) => {
            error!(?error, "eframe encountered an error");
        }
    }
}
