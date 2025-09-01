# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

### Build and Run
```bash
cargo build              # Build the project
cargo build --release    # Build optimized release version
cargo run                # Run the application
cargo check              # Quick compilation check
```

### Development
```bash
cargo fmt                # Format code
cargo clippy             # Run linter
```

## Architecture Overview

Selara is a real-time audio spectrum analyzer with a threaded architecture designed for low-latency audio processing:

### Core Data Flow
1. **Audio Capture** (`audio.rs:build_loopback_stream`) - Captures system audio via `cpal` loopback stream
2. **Signal Processing** (`audio.rs:start_spectrum_analyzer`) - FFT analysis thread processes audio frames
3. **UI Rendering** (`main.rs` loop) - Terminal interface updates at ~60 FPS

### Threading Model
- **Main Thread**: UI event handling and rendering loop
- **Audio Callback Thread**: Real-time audio capture (system audio thread priority)  
- **FFT Processing Thread**: Spectrum analysis with windowing and frequency band mapping

### Key Data Structures
- `Meter` - RMS and peak audio levels
- `Spectrum` - Frequency band data (both dB and linear scales)
- `App` - UI state including spectrum data, device info, and display mode

### Inter-Thread Communication
Uses `crossbeam-channel` for lock-free message passing:
- `rx_frames/tx_frames` - Raw audio frames from capture to FFT thread
- `rx/tx_meter` - Audio level data to UI
- `rx_spec/tx_spec` - Spectrum data to UI

### Audio Processing Details
- FFT size: 1024 samples with 50% overlap (hop = 512)
- Windowing: Hann window applied before FFT
- Frequency mapping: Logarithmic scale from 20Hz to 20kHz across 96 bands
- Smoothing: Exponential smoothing (alpha = 0.6) for stable visualization

### UI Architecture
- Built with `ratatui` for cross-platform terminal interface
- Event-driven with keyboard controls (L for mode toggle, q/ESC to quit)
- Dual display modes: dB scale (-60dB to 0dB) and linear magnitude
- Peak hold functionality with time-based decay
