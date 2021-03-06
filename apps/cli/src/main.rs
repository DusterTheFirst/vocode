#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used)]

use color_eyre::{self, eyre::Context};
use rodio::{buffer::SamplesBuffer, OutputStream, Sink};
use std::io::{self, Write};
use tts::{load_language, setup_tts, synthesize};

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    let resources = loop {
        write!(stdout, "language> ").wrap_err("unable to write to stdout")?;
        stdout.flush().wrap_err("unable to write to stdout")?;

        let mut lang = String::new();
        stdin
            .read_line(&mut lang)
            .wrap_err("unable to read input line")?;

        let lang = lang.trim_end();

        match load_language(lang) {
            Ok(resources) => break resources,
            Err(error) => eprintln!("{}", error),
        }
    };

    let mut engine = setup_tts(resources)?;

    let (_stream, stream_handle) =
        OutputStream::try_default().wrap_err("unable to open audio output stream")?;
    let sink = Sink::try_new(&stream_handle).wrap_err("unable to create sink")?;

    loop {
        write!(stdout, "synth> ").wrap_err("unable to write to stdout")?;
        stdout.flush().wrap_err("unable to write to stdout")?;

        let mut line = String::new();
        stdin
            .read_line(&mut line)
            .wrap_err("unable to read input line")?;

        let line = line.trim_end();

        let waveform = synthesize(&mut engine, line)?;

        sink.append(SamplesBuffer::new(
            1,
            waveform.sample_rate(),
            waveform.samples(),
        ));

        sink.sleep_until_end();
    }
}
