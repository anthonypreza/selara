use cpal::traits::{DeviceTrait, HostTrait};
use crossbeam_channel as chan;
use std::time::{Duration, Instant};

mod audio;
mod types;
mod ui;

use audio::{create_audio_stream, start_spectrum_analyzer};
use types::{Meter, Spectrum};
use ui::{App, draw_ui, handle_events, init_terminal, restore_terminal};

fn main() -> Result<(), anyhow::Error> {
    let mut terminal = init_terminal()?;

    let cleanup = || {
        let _ = restore_terminal();
    };

    ctrlc::set_handler(move || {
        cleanup();
        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    let host = cpal::default_host();

    let default_out = host
        .default_output_device()
        .expect("Failed to get default output device");

    let device_name = default_out
        .name()
        .unwrap_or_else(|_| "Unknown Device".to_string());

    let output_cfg = match default_out.default_output_config() {
        Ok(f) => f,
        Err(e) => {
            panic!("Error getting default output stream: {:?}", e)
        }
    };

    let cfg = output_cfg.config();
    let channels = cfg.channels as usize;
    let (tx_meter, rx) = chan::bounded::<Meter>(32);
    let (tx_spec, rx_spec) = chan::bounded::<Spectrum>(8);
    let sample_rate = cfg.sample_rate.0 as f32;

    let (tx_frames, rx_frames) = chan::bounded::<Vec<f32>>(16);

    // Start spectrum analyzer thread
    start_spectrum_analyzer(rx_frames, tx_spec, sample_rate);

    // Create audio stream
    let _stream = create_audio_stream(
        &default_out,
        output_cfg.sample_format(),
        &cfg,
        channels,
        tx_meter.clone(),
        tx_frames.clone(),
    )?;

    let mut app = App::new(sample_rate as u32, device_name);
    let frame_duration = Duration::from_millis(16); // ~60 FPS
    let mut last_time = Instant::now();

    loop {
        let now = Instant::now();
        let dt = now.duration_since(last_time).as_secs_f32();
        last_time = now;

        app.decay_peak(dt);

        if let Ok(spec) = rx_spec.try_recv() {
            app.update_spectrum(spec);
        }

        if let Ok(meter) = rx.try_recv() {
            app.update_rms(meter.rms);
        }

        handle_events(&mut app)?;

        if app.should_quit {
            break;
        }

        terminal.draw(|f| draw_ui(f, &app))?;

        std::thread::sleep(frame_duration);
    }

    restore_terminal()?;
    Ok(())
}
