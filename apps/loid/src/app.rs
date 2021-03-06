#![forbid(unsafe_code)]

use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Once,
    },
    time::Duration,
};

use audio::{
    output::{AudioSink, AudioSinkProgress},
    waveform::Waveform,
};
use eframe::{
    egui::{
        Button, CentralPanel, Context, RichText, ScrollArea, SidePanel, Slider, TopBottomPanel,
    },
    epaint::Vec2,
    epi::{App, Frame},
};
use instant::Instant;
use spectrum::{WaveformSpectrum, Window};
use tracing::warn;

mod plot;

pub struct Application {
    math_elapsed: Option<Duration>,

    audio_sink: AudioSink,

    waveform: Option<Waveform<'static>>,
    window: Window,

    is_playing: Arc<AtomicBool>,
    playback_head: Arc<AtomicUsize>,

    follow_playback: bool,
    full_spectrum: bool,
    phase: bool,
    decibels: bool,
    line: bool,
    stems: bool,

    cursor: usize,
    fft_width: u8,
    window_width: usize,
    hop_frac: usize,

    shift: f64,
}

impl Application {
    pub fn new(audio_sink: AudioSink) -> Self {
        Self {
            math_elapsed: None,

            audio_sink,

            waveform: None,

            window: Window::Hann,

            is_playing: Arc::new(AtomicBool::new(false)),
            playback_head: Arc::new(AtomicUsize::new(0)),

            follow_playback: true,
            full_spectrum: false,
            phase: false,
            decibels: false,

            // Use line plot on wasm32 platforms
            line: cfg!(target_arch = "wasm32"),
            stems: true,

            cursor: 0,

            // TODO: Better defaults
            fft_width: 11,
            window_width: 2048,
            hop_frac: 4,

            shift: 0.0,
        }
    }
}

impl Application {
    // pub fn initialize() -> color_eyre::Result<Self> {
    // let (_stream, stream_handle) = OutputStream::try_default().unwrap();

    // let sink = Sink::try_new(&stream_handle).unwrap();

    // let resources = load_language("en-US").unwrap();

    // let mut engine = setup_tts(resources).wrap_err("unable to setup tts engine")?;

    // let speech = synthesize(&mut engine, "Some Body Once").wrap_err("unable to synthesize text")?;
    // let speech = SineWave::new(120.0).take_duration(Duration::from_millis(300));

    // let sample_rate = speech.sample_rate();
    // let samples: Vec<f32> = speech.convert_samples().collect();

    // let (samples, SampleRate(sample_rate)) = audio::input::h()?;
    // }

    // fn reconstruct_samples(&mut self) {
    //     self.reconstructed_samples.clear();

    //     let mut window_samples = Vec::new();

    //     for window_start in (0..self.samples.len()).step_by(self.width) {
    //         if window_start + self.width >= self.samples.len() {
    //             let window = window_start..window_start + self.width;
    //             warn!(?window, "skipping window");

    //             break;
    //         }

    //         spectrum(window_start, self.width, &self.samples, &mut self.spectrum);
    //         if self.is_scale {
    //             todo!();
    //             // scale_spectrum(spectrum, &mut self.shifted_spectrum, self.shift);

    //             // self.shifted_spectrum[0] = Complex::new(0.0, 0.0);
    //         } else {
    //             shift_spectrum(
    //                 self.bucket_from_freq(self.shift),
    //                 &self.spectrum,
    //                 &mut self.shifted_spectrum,
    //             )
    //         }

    //         reconstruct_samples(
    //             &self.shifted_spectrum,
    //             &mut self.reconstructed_work_buffer,
    //             &mut window_samples,
    //             self.width,
    //         );

    //         self.reconstructed_samples.append(&mut window_samples);

    //         // self.shift += 500.0 * (self.width as f64 / self.samples.len() as f64) as f64;
    //     }
    // }

    // FIXME: broken on web
    fn play(&self, waveform: &Waveform<'_>, ctx: Context) {
        tracing::info!(
            "Playing {} samples ({} seconds)",
            waveform.len(),
            waveform.duration()
        );

        let did_queue = self.audio_sink.queue(waveform, {
            let playback_head = self.playback_head.clone();
            let is_playing = self.is_playing.clone();
            let waveform_len = waveform.len() as f32;

            let once = Once::new();

            move |progress| {
                match progress {
                    AudioSinkProgress::Samples(progress) => {
                        once.call_once(|| {
                            is_playing.store(true, Ordering::SeqCst);
                        });

                        playback_head
                            .store((progress * waveform_len).round() as _, Ordering::SeqCst);
                    }
                    AudioSinkProgress::Finished => {
                        playback_head.store(waveform_len as _, Ordering::SeqCst);
                        is_playing.store(false, Ordering::SeqCst);
                    }
                };
                ctx.request_repaint();
            }
        });

        if !did_queue {
            warn!("Failed to queue waveform");
        }
    }
}

impl App for Application {
    fn persist_native_window(&self) -> bool {
        false
    }

    fn persist_egui_memory(&self) -> bool {
        false
    }

    fn update(&mut self, ctx: &Context, frame: &mut Frame) {
        TopBottomPanel::top("nav_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                eframe::egui::widgets::global_dark_light_mode_switch(ui);
                ui.menu_button("Waveform", |ui| {
                    if ui.button("Load Sine Wave").clicked() {
                        self.waveform =
                            Some(Waveform::sine_wave(220.0, 0.5, Waveform::CD_SAMPLE_RATE));

                        ui.close_menu();
                    }
                    ui.separator();
                    if ui
                        .add_enabled(self.waveform.is_some(), Button::new("Clear"))
                        .clicked()
                    {
                        self.waveform = None;

                        ui.close_menu();
                    }
                });
            });
        });

        SidePanel::left("left_panel").show(ctx, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                ui.heading("Rendering Statistics");
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;

                    ui.label("Last math: ");
                    ui.label(
                        RichText::new(
                            self.math_elapsed
                                .map(|duration| {
                                    format!("{:.4} ms", duration.as_nanos() as f32 / 1_000_000.0)
                                })
                                .unwrap_or_else(|| "N/A".to_string()),
                        )
                        .monospace(),
                    );
                });
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;

                    ui.label("Last frame: ");
                    ui.label(
                        RichText::new(format!(
                            "{:.4}",
                            frame.info().cpu_usage.unwrap_or(0.0) * 1000.0
                        ))
                        .monospace(),
                    );
                    ui.label(" ms");
                });
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;

                    ui.label("Max refresh: ");
                    ui.label(
                        RichText::new(format!(
                            "{:.4}",
                            1.0 / frame.info().cpu_usage.unwrap_or(0.0)
                        ))
                        .monospace(),
                    );
                    ui.label(" fps");
                });

                ui.separator();
                ui.heading("Playback");
                ui.add_enabled_ui(
                    self.waveform.is_some() && !self.audio_sink.playing(),
                    |ui| {
                        if ui.button("Play Original").clicked() {
                            self.play(
                                self.waveform
                                    .as_ref()
                                    .expect("button cannot be pressed with no waveform"),
                                ctx.clone(),
                            );
                        }

                        if ui
                            .add_enabled(
                                false,
                                // self.audio_sink.empty() && !self.reconstructed_samples.is_empty(),
                                Button::new("Play Reconstructed"),
                            )
                            .clicked()
                        {
                            // self.play(self.reconstructed_samples.as_ref(), frame.clone());
                        }
                    },
                );

                if ui
                    .add_enabled(false, Button::new("Reconstruct Samples"))
                    .clicked()
                {
                    // self.reconstruct_samples();
                }

                ui.checkbox(&mut self.follow_playback, "FFT follows playback");

                ui.separator();
                // TODO: disable during playback?
                ui.add_enabled_ui(true, |ui| {
                    // TODO: better handling of no waveform
                    let waveform_len = self.waveform.as_ref().map(|w| w.len()).unwrap_or(0);

                    ui.heading("FFT");
                    ui.label("FFT Width");
                    ui.add(
                        Slider::new(
                            &mut self.fft_width,
                            1..=((waveform_len.next_power_of_two().trailing_zeros() as u8)
                                .saturating_sub(1)
                                .max(1)),
                        )
                        .prefix("2^")
                        .suffix(" samples"),
                    );

                    // Ensure the window width is always <= fft_width
                    self.window_width = self.window_width.min(1 << self.fft_width);

                    ui.label("Window Width");
                    ui.add(
                        Slider::new(&mut self.window_width, 2..=(1 << self.fft_width))
                            .suffix(" samples"),
                    );

                    ui.label("Window Function");
                    ui.horizontal_wrapped(|ui| {
                        for window in Window::ALL {
                            ui.selectable_value(&mut self.window, window, window.to_string());
                        }
                    });

                    ui.label("Hop Fraction");
                    ui.add(
                        Slider::new(&mut self.hop_frac, 1..=16)
                            .prefix("1/")
                            .logarithmic(true),
                    );

                    let max_cursor = waveform_len.saturating_sub(self.window_width + 1);
                    self.cursor = self.cursor.min(max_cursor);

                    ui.label("Window Start");
                    ui.add(Slider::new(&mut self.cursor, 0..=max_cursor).prefix("sample "));

                    ui.horizontal_wrapped(|ui| {
                        let step = self.window_width / self.hop_frac;

                        if ui
                            .add_enabled(self.cursor >= step, Button::new("Previous"))
                            .clicked()
                        {
                            self.cursor -= step;
                        }

                        if ui
                            .add_enabled(
                                self.cursor + self.window_width + step <= waveform_len,
                                Button::new("Next"),
                            )
                            .clicked()
                        {
                            self.cursor += step;
                        }
                    });

                    ui.separator();
                    ui.heading("DSP");
                    ui.label("Frequency shift");
                    ui.add(Slider::new(&mut self.shift, 0.0..=1000.0).suffix(" Hz"));
                });

                ui.separator();
                ui.heading("Visualization");
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut self.full_spectrum, "Show full spectrum");
                    ui.checkbox(&mut self.phase, "Show phase");
                    ui.checkbox(&mut self.decibels, "Decibels");
                    ui.checkbox(&mut self.line, "Line Plot");
                    ui.checkbox(&mut self.stems, "Stems");
                });

                ui.separator();
                ui.heading("Debug");
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Panic").clicked() {
                        panic!("User initiated panic");
                    }
                });

                // About section
                ui.separator();

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;

                    ui.label("powered by ");
                    ui.hyperlink_to("egui", "https://github.com/emilk/egui");
                    ui.label(" and ");
                    ui.hyperlink_to("eframe", "https://github.com/emilk/egui/tree/master/eframe");
                });

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;

                    ui.label("source available on ");
                    ui.hyperlink_to("github", "https://github.com/dusterthefirst/vocode");
                });
            });
        });

        if let Some(waveform) = &self.waveform {
            let cursor = if self.follow_playback && self.is_playing.load(Ordering::SeqCst) {
                self.playback_head
                    .load(Ordering::SeqCst)
                    .min(waveform.len() - self.window_width - 1)
            } else {
                self.cursor
            };

            // Calculate FFT width in bytes
            let fft_width = 1 << self.fft_width;

            let math_start = Instant::now();

            // Get the slice of the waveform to work on
            let window_waveform = waveform.slice(cursor..(cursor + self.window_width));

            // Get the frequency spectrum of the waveform
            let spectrum = window_waveform.spectrum(self.window, fft_width);

            // Shift the spectrum
            let shifted_spectrum = spectrum.shift(spectrum.bucket_from_freq(self.shift));

            let reconstructed = shifted_spectrum.waveform();
            let reconstructed = reconstructed.slice(..self.window_width);

            self.math_elapsed = Some(math_start.elapsed());

            TopBottomPanel::top("top_panel").show(ctx, |ui| {
                ui.label(format!(
                    "Frequency Resolution: {} Hz",
                    spectrum.freq_resolution()
                ));

                ui.label(format!("FFT algorithm: cfft_{}", fft_width));
            });

            CentralPanel::default().show(ctx, |ui| {
                let plot_size = ui.available_size();
                let plot_size = Vec2::new(plot_size.x, plot_size.y / 3.0);

                ui.allocate_ui(plot_size, |ui| {
                    plot::waveform_display(
                        ui,
                        waveform,
                        cursor,
                        self.playback_head.load(Ordering::SeqCst),
                        self.window_width,
                        self.hop_frac,
                        (self.line, self.stems),
                    )
                });
                ui.allocate_ui(plot_size, |ui| {
                    plot::window_display(
                        ui,
                        &window_waveform,
                        (self.window, self.window_width),
                        &reconstructed,
                        self.hop_frac,
                        (self.line, self.stems),
                    )
                });
                ui.allocate_ui(plot_size, |ui| {
                    plot::spectrum_display(
                        ui,
                        &spectrum,
                        &shifted_spectrum,
                        self.full_spectrum,
                        self.phase,
                        self.decibels,
                    )
                })
            });
        } else {
            CentralPanel::default().show(ctx, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.heading("No Waveform Loaded");
                });
            });
        }
    }
}
