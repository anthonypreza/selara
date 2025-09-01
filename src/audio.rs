use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{
    Device, FromSample, InputCallbackInfo, Sample, SampleFormat, SizedSample, Stream, StreamConfig,
    StreamError,
};
use crossbeam_channel::Sender;
use realfft::RealFftPlanner;
use realfft::num_complex::Complex32;
use std::time::Duration;

use crate::types::{Meter, Spectrum};

pub fn start_spectrum_analyzer(
    rx_frames: crossbeam_channel::Receiver<Vec<f32>>,
    tx_spec: Sender<Spectrum>,
    sample_rate: f32,
) {
    std::thread::spawn(move || {
        // FFT setup
        let fft_size: usize = 1024;
        let hop: usize = fft_size / 2;
        let bands_target: usize = 96;
        let smoothing_alpha: f32 = 0.6;

        let mut planner = RealFftPlanner::<f32>::new();
        let r2c = planner.plan_fft_forward(fft_size);

        let mut input: Vec<f32> = r2c.make_input_vec();
        let mut spectrum: Vec<Complex32> = r2c.make_output_vec();
        let mut scratch = r2c.make_scratch_vec();

        let window: Vec<f32> = (0..fft_size)
            .map(|i| {
                let n = i as f32;
                0.5 - 0.5 * ((2.0 * std::f32::consts::PI * n) / fft_size as f32).cos()
            })
            .collect();

        let num_bins = spectrum.len(); // == fft_size/2 + 1
        let bands = bands_target;
        let f_lo = 20.0f32;
        let f_hi = (sample_rate / 2.0).min(20_000.0);
        let bin_hz = |bin: usize| (bin as f32) * sample_rate / (fft_size as f32);

        let mut bin_to_band = vec![0usize; num_bins];
        for bin in 0..num_bins {
            let f = bin_hz(bin).max(f_lo);
            let t = ((f / f_lo).ln() / (f_hi / f_lo).ln()).clamp(0.0, 1.0);
            let b = (t * (bands as f32 - 1.0)).round() as usize;
            bin_to_band[bin] = b.min(bands - 1);
        }

        // smoothing buffer
        let mut smooth = vec![0.0f32; bands];
        let mut smooth_linear = vec![0.0f32; bands];

        // rolling buffer of mono frames
        let mut ring: Vec<f32> = Vec::with_capacity(fft_size * 2);

        let gain = 0.2;

        while let Ok(chunk) = rx_frames.recv() {
            // append new frames from callback
            ring.extend_from_slice(&chunk);

            // process as long as we have one full FFT frame
            while ring.len() >= fft_size {
                // copy + window (no alloc inside the loop)
                for i in 0..fft_size {
                    input[i] = ring[i] * window[i];
                }

                // FFT
                r2c.process_with_scratch(&mut input, &mut spectrum, &mut scratch)
                    .expect("FFT failed");

                // magnitude → bands
                let mut bands_pow = vec![0.0f32; bands];
                let mut bands_cnt = vec![0u32; bands];

                for (bin, c) in spectrum.iter().enumerate() {
                    let mag2 = c.re * c.re + c.im * c.im; // power
                    let b = bin_to_band[bin];
                    bands_pow[b] += mag2;
                    bands_cnt[b] += 1;
                }

                // average + compression + smoothing
                for b in 0..bands {
                    let p = if bands_cnt[b] > 0 {
                        bands_pow[b] / (bands_cnt[b] as f32)
                    } else {
                        0.0
                    };

                    // Linear magnitude for linear mode
                    let linear_level = if p > 0.0 {
                        let magnitude = p.sqrt();
                        (magnitude * gain * 0.8).clamp(0.0, 1.0) // Lower gain for more dynamics
                    } else {
                        0.0
                    };

                    // Convert to decibels with proper reference
                    let db_level = if p > 0.0 {
                        let magnitude = p.sqrt();
                        let db = 20.0 * (magnitude * gain).log10();
                        // Map from -60dB to 0dB range to 0.0-1.0
                        ((db + 60.0) / 60.0).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };

                    smooth[b] = smoothing_alpha * db_level + (1.0 - smoothing_alpha) * smooth[b];
                    smooth_linear[b] =
                        smoothing_alpha * linear_level + (1.0 - smoothing_alpha) * smooth_linear[b];
                }

                // send latest smoothed bands
                let _ = tx_spec.try_send(Spectrum {
                    bands: smooth.clone(),
                    bands_linear: smooth_linear.clone(),
                });

                // advance by hop (50% overlap)
                ring.drain(0..hop);
            }
        }
    });
}

pub fn build_loopback_stream<T>(
    device: &Device,
    cfg: &StreamConfig,
    channels: usize,
    tx_meter: Sender<Meter>,
    tx_frames: Sender<Vec<f32>>,
) -> Result<Stream, anyhow::Error>
where
    T: Sample + Send + 'static + SizedSample + std::fmt::Debug,
    f32: FromSample<<T as Sample>::Float>,
{
    let err_callback = |err: StreamError| eprintln!("an error occurred on stream: {}", err);

    let input_callback = move |data: &[T], _info: &InputCallbackInfo| {
        // Convert interleaved frames to mono f32
        let mut rms_acc: f32 = 0.0;
        let mut peak: f32 = 0.0;
        let mut n: usize = 0;
        let mut mono_chunk = Vec::with_capacity(data.len() / channels);

        // Process per-frame without allocation
        for frame in data.chunks(channels) {
            let left = frame
                .first()
                .map(|s| f32::from_sample(s.to_float_sample()))
                .unwrap_or(0.0f32);

            let right = if channels > 1 {
                frame
                    .get(1)
                    .map(|s| f32::from_sample((*s).to_float_sample()))
                    .unwrap_or(0.0f32)
            } else {
                0.0f32
            };

            let mono = 0.5f32 * (left + right);
            mono_chunk.push(mono);

            let a = mono.abs();
            if a > peak {
                peak = a;
            }
            rms_acc += mono * mono;
            n += 1
        }

        if n > 0 {
            let rms = (rms_acc / n as f32).sqrt();
            let _ = tx_meter.try_send(Meter { rms, peak });
            let _ = tx_frames.try_send(mono_chunk);
        }
    };

    let latency = Some(Duration::from_millis(20));
    let stream = device.build_input_stream(cfg, input_callback, err_callback, latency)?;
    stream.play()?;
    Ok(stream)
}

pub fn create_audio_stream(
    device: &Device,
    sample_format: SampleFormat,
    cfg: &StreamConfig,
    channels: usize,
    tx_meter: Sender<Meter>,
    tx_frames: Sender<Vec<f32>>,
) -> Result<Stream, anyhow::Error> {
    match sample_format {
        SampleFormat::F32 => {
            build_loopback_stream::<f32>(device, cfg, channels, tx_meter, tx_frames)
        }
        SampleFormat::I16 => {
            build_loopback_stream::<i16>(device, cfg, channels, tx_meter, tx_frames)
        }
        SampleFormat::U16 => {
            build_loopback_stream::<u16>(device, cfg, channels, tx_meter, tx_frames)
        }
        _ => {
            panic!("Unsupported sample format: {:?}", sample_format)
        }
    }
}
