#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![warn(missing_copy_implementations, missing_debug_implementations)]

use std::{
    cmp::Ordering,
    f32::consts,
    fmt::{self, Display},
    iter,
    ops::Range,
};

use audio::waveform::Waveform;
pub use num_complex::Complex;

mod fft;

use fft::cfft;

// pub fn pitch_change(samples: &[f32])

#[deprecated]
pub fn reconstruct_samples(
    full_spectrum: &[Complex<f32>],
    work_buffer: &mut Vec<Complex<f32>>,
    samples: &mut Vec<f32>,
    width: usize,
) {
    debug_assert_eq!(
        full_spectrum.len(),
        width,
        "full spectrum width does not match fft width"
    );

    work_buffer.clear();
    work_buffer.extend(
        full_spectrum
            .iter()
            .map(|complex| Complex::new(complex.im, complex.re)),
    );
    samples.shrink_to_fit();

    cfft(work_buffer);

    samples.clear();
    samples.extend(work_buffer.iter().map(|complex| complex.im / width as f32));
    samples.shrink_to_fit();
}

// TODO: signed shift?
#[deprecated]
pub fn shift_spectrum(
    buckets: usize,

    spectrum: &[Complex<f32>],
    shifted_spectrum: &mut Vec<Complex<f32>>,
) {
    shifted_spectrum.clear();

    // If the result would shift all components off, take a shortcut and just fill it with zeros
    if buckets >= spectrum.len() / 2 {
        shifted_spectrum.resize(spectrum.len(), Complex::new(0.0, 0.0));
        return;
    }

    let zero_iter = iter::repeat(Complex::new(0.0, 0.0)).take(buckets);
    let half_spectrum_length = spectrum.len() / 2 - buckets;

    let (second_half_skip, second_zero_skip) = if buckets == 0 { (1, 0) } else { (0, 1) };

    shifted_spectrum.extend(
        zero_iter
            .clone()
            .chain(spectrum.iter().copied().take(half_spectrum_length + 1))
            .chain(
                spectrum
                    .iter()
                    .map(Complex::conj)
                    .take(half_spectrum_length)
                    .skip(second_half_skip)
                    .rev(),
            )
            .chain(zero_iter.skip(second_zero_skip)),
    );
}

#[deprecated]
pub fn scale_spectrum(
    scale: f32,

    spectrum: &[Complex<f32>],
    scaled_spectrum: &mut Vec<Complex<f32>>,
) {
    scaled_spectrum.clear();
    scaled_spectrum.resize(spectrum.len(), Complex::new(0.0, 0.0));
    scaled_spectrum.shrink_to_fit();

    let width = spectrum.len();
    let half_width = width / 2 + 1;

    // Copy DC offset
    scaled_spectrum[0].re = spectrum[0].re;

    // TODO: do something about the nyquist frequency (imaginary component of DC)

    // Iterate over all real frequencies, saving them into the new spectrum
    for (bucket, component) in spectrum
        .iter()
        .take(half_width)
        .copied()
        .enumerate()
        .skip(1)
    {
        // TODO: non-integer scaling
        let bucket = (bucket as f32 * scale).round() as usize;

        if bucket > half_width {
            break;
        }

        // TODO: way to let the compiler know bounds checks are not needed?
        scaled_spectrum[bucket] = component;
    }

    // Split the spectrum at one over half since 1-nyquist is shared between the two
    let (original, mirror) = scaled_spectrum.split_at_mut(half_width);

    // Skip the DC offset which is only present in the left hand side
    let original = original.iter().skip(1);

    // Reverse the order that we iterate through the mirror
    let mirror = mirror.iter_mut().rev();

    // Mirror changes to other half of spectrum
    for (original, mirror) in original.zip(mirror) {
        // let gamma = scale * original.arg() * TODO:
        *mirror = original.conj();
    }
}

#[derive(Debug)]
pub struct Spectrum<'waveform> {
    width: usize,
    buckets: Box<[Complex<f32>]>,
    waveform: &'waveform Waveform<'waveform>,
}

impl<'w> Spectrum<'w> {
    pub fn width(&self) -> usize {
        self.width
    }

    pub fn buckets(&self) -> &[Complex<f32>] {
        &self.buckets
    }

    pub fn amplitudes(&self) -> impl Iterator<Item = f32> + '_ {
        self.buckets.iter().map(|complex| complex.norm())
    }

    pub fn phases(&self) -> impl Iterator<Item = f32> + '_ {
        self.buckets
            .iter()
            .map(|complex| complex.arg() / self.width as f32)
    }

    pub fn amplitudes_real(&self) -> impl Iterator<Item = f32> + '_ {
        self.amplitudes().take(self.width / 2 + 1)
    }

    pub fn phases_real(&self) -> impl Iterator<Item = f32> + '_ {
        self.phases().take(self.width / 2 + 1)
    }

    // TODO: rename?
    pub fn main_frequency(&self) -> Option<(usize, f32)> {
        self.amplitudes_real()
            .enumerate()
            .max_by(|&(_, amp_1), &(_, amp_2)| {
                amp_1.partial_cmp(&amp_2).unwrap_or_else(|| {
                    // Choose the non-nan value
                    match (amp_1.is_nan(), amp_2.is_nan()) {
                        (true, true) => panic!("encountered two NaN values"),
                        (false, true) => Ordering::Greater,
                        (true, false) => Ordering::Less,
                        (false, false) => unreachable!(),
                    }
                })
            })
    }

    pub fn freq_resolution(&self) -> f64 {
        (1.0 / self.width as f64) * self.waveform.sample_rate() as f64
    }

    pub fn freq_from_bucket(&self, bucket: usize) -> f64 {
        if bucket > self.width / 2 {
            -((self.width - bucket) as f64 * self.freq_resolution())
        } else {
            bucket as f64 * self.freq_resolution()
        }
    }

    pub fn bucket_from_freq(&self, freq: f64) -> usize {
        ((freq * self.width as f64) / self.waveform.sample_rate() as f64).round() as usize
    }

    // TODO: signed shift?
    #[must_use = "shift creates a new spectrum"]
    pub fn shift(&self, shift: usize) -> Spectrum<'w> {
        let half_spectrum = self.width / 2;

        Spectrum {
            width: self.width,
            waveform: self.waveform,
            buckets: iter::repeat(Complex::new(0.0, 0.0))
                .take(shift)
                .chain(self.buckets[..(half_spectrum - shift)].iter().copied())
                .chain(self.buckets[(half_spectrum + shift)..].iter().copied())
                .chain(iter::repeat(Complex::new(0.0, 0.0)).take(shift))
                .collect(),
        }
    }

    #[must_use]
    pub fn waveform(&self) -> Waveform<'static> {
        let mut spectrum = self
            .buckets
            .iter()
            .map(|complex| Complex::new(complex.im, complex.re))
            .collect::<Vec<_>>();

        cfft(&mut spectrum);

        Waveform::new(
            spectrum
                .into_iter()
                .map(|complex| complex.im / self.width as f32)
                .collect(),
            self.waveform.sample_rate(),
        )
    }
}

mod sealed {
    pub trait Sealed {}
}

impl<'w> sealed::Sealed for Waveform<'w> {}

pub trait WaveformSpectrum: sealed::Sealed {
    #[must_use]
    fn spectrum(&self, window: Window, fft_width: usize) -> Spectrum;
}

impl<'w> WaveformSpectrum for Waveform<'w> {
    // TODO: see if rfft would be worth using unsafe for over cfft
    #[must_use]
    fn spectrum(&self, window: Window, fft_width: usize) -> Spectrum {
        assert!(
            self.len() <= fft_width,
            "{} is too many samples for a fft of width {fft_width}",
            self.len()
        );
        assert!(
            fft_width.is_power_of_two(),
            "fft width length must be a power of two"
        );

        let window = window.into_iter(self.len());

        // Copy samples into the spectrum, filling any extra space with zeros
        let mut buckets = self
            .samples_iter()
            .zip(window)
            .map(|(sample, scale)| Complex::new(sample * scale, 0.0))
            .chain(iter::repeat(Complex::new(0.0, 0.0)))
            .take(fft_width)
            .collect::<Box<_>>();

        // Perform the FFT based on the calculated width
        cfft(&mut buckets);

        Spectrum {
            buckets,
            width: fft_width,
            waveform: self,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Window {
    #[doc(alias = "Triangular")]
    Bartlett,
    Hamming,
    /// Good default choice
    Hann,
    Rectangular,
}

impl Display for Window {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Window {
    pub const ALL: [Window; 4] = [Self::Bartlett, Self::Hamming, Self::Hann, Self::Rectangular];

    pub fn into_iter(self, width: usize) -> WindowIter {
        WindowIter {
            range: 0..width,
            width,
            window: self,
        }
    }
}

#[derive(Debug)]
pub struct WindowIter {
    range: Range<usize>,
    width: usize,
    window: Window,
}

impl Iterator for WindowIter {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(n) = self.range.next() {
            let n = n as f32;
            let width = self.width as f32;

            Some(match self.window {
                Window::Rectangular => 1.0,
                Window::Bartlett => 1.0 - f32::abs((n - width / 2.0) / (width / 2.0)),
                Window::Hann => 0.5 * (1.0 - f32::cos((consts::TAU * n) / width)),
                Window::Hamming => {
                    (25.0 / 46.0) - ((21.0 / 46.0) * f32::cos((consts::TAU * n) / width))
                }
            })
        } else {
            None
        }
    }
}
