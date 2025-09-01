use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Bar, BarChart, BarGroup, Block, Borders, Gauge, Paragraph},
};
use std::{
    io::{self, Stdout},
    time::Duration,
};

use crate::types::Spectrum;

pub struct App {
    pub should_quit: bool,
    pub last_rms: f32,
    pub peak_hold: f32,
    pub last_spectrum: Option<Spectrum>,
    pub sample_rate: u32,
    pub device_name: String,
    pub linear_mode: bool,
}

impl App {
    pub fn new(sample_rate: u32, device_name: String) -> App {
        App {
            should_quit: false,
            last_rms: 0.0,
            peak_hold: 0.0,
            last_spectrum: None,
            sample_rate,
            device_name,
            linear_mode: false, // Start with dB mode
        }
    }

    pub fn update_rms(&mut self, rms: f32) {
        self.last_rms = rms;
        self.peak_hold = self.peak_hold.max(rms);
    }

    pub fn update_spectrum(&mut self, spectrum: Spectrum) {
        self.last_spectrum = Some(spectrum);
    }

    pub fn decay_peak(&mut self, dt: f32) {
        let decay_per_sec = 0.90f32;
        self.peak_hold *= decay_per_sec.powf(dt);
    }
}

pub type TerminalType = Terminal<CrosstermBackend<Stdout>>;

pub fn init_terminal() -> Result<TerminalType, anyhow::Error> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

pub fn restore_terminal() -> Result<(), anyhow::Error> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
    Ok(())
}

pub fn handle_events(app: &mut App) -> Result<(), anyhow::Error> {
    if event::poll(Duration::from_millis(0))? {
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        app.should_quit = true;
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.should_quit = true;
                    }
                    KeyCode::Char('l') | KeyCode::Char('L') => {
                        app.linear_mode = !app.linear_mode;
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn create_color_gradient(position: f32) -> Color {
    let pos = position.clamp(0.0, 1.0);

    if pos < 0.33 {
        let t = pos / 0.33;
        let r = (64.0 + (128.0 - 64.0) * t) as u8;
        let g = (224.0 + (160.0 - 224.0) * t) as u8;
        let b = (208.0 + (128.0 - 208.0) * t) as u8;
        Color::Rgb(r, g, b)
    } else if pos < 0.66 {
        let t = (pos - 0.33) / 0.33;
        let r = (128.0 + (64.0 - 128.0) * t) as u8;
        let g = (160.0 + (224.0 - 160.0) * t) as u8;
        let b = (128.0 + (224.0 - 128.0) * t) as u8;
        Color::Rgb(r, g, b)
    } else {
        let t = (pos - 0.66) / 0.34;
        let r = (64.0 + (128.0 - 64.0) * t) as u8;
        let g = (224.0 + (96.0 - 224.0) * t) as u8;
        let b = (224.0 + (160.0 - 224.0) * t) as u8;
        Color::Rgb(r, g, b)
    }
}

pub fn draw_ui(f: &mut Frame, app: &App) {
    let size = f.area();

    // Check if the terminal is too small to display the UI properly
    if size.width < 30 || size.height < 17 {
        let error_msg = Paragraph::new("Terminal too small!\nMinimum: 30x17")
            .style(Style::default().fg(Color::Red))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(error_msg, size);
        return;
    }

    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Length(4), // RMS meter
            Constraint::Min(10),   // EQ spectrum
            Constraint::Length(3), // Frequency labels
            Constraint::Length(5), // Status bar
        ])
        .split(size);

    draw_title(f, main_layout[0]);
    draw_rms_meter(f, main_layout[1], app);
    draw_eq_spectrum(f, main_layout[2], app);
    draw_frequency_labels(f, main_layout[3], app);
    draw_status_bar(f, main_layout[4], app);
}

fn draw_title(f: &mut Frame, area: Rect) {
    let title = Paragraph::new("🎵 Selara Audio Visualizer")
        .style(
            Style::default()
                .fg(Color::Rgb(128, 224, 208))
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Rgb(96, 160, 192))),
        );
    f.render_widget(title, area);
}

fn draw_rms_meter(f: &mut Frame, area: Rect, app: &App) {
    let rms_block = Block::default()
        .title(" RMS Level ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(96, 160, 192)));

    let inner = rms_block.inner(area);
    f.render_widget(rms_block, area);

    let gain = 2.0f32;
    let level = (app.last_rms * gain).clamp(0.0, 1.0);
    let peak_level = (app.peak_hold * gain).clamp(0.0, 1.0);

    let gauge_color = create_color_gradient(level);

    let rms_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(inner);

    // Text above the gauge
    let rms_text = Paragraph::new(format!(
        "RMS: {:.3} | Peak: {:.3}",
        app.last_rms, app.peak_hold
    ))
    .style(Style::default().fg(Color::Rgb(200, 200, 200)))
    .alignment(Alignment::Center);
    f.render_widget(rms_text, rms_layout[0]);

    // Gauge without label or percentage
    let gauge = Gauge::default()
        .block(Block::default())
        .gauge_style(Style::default().fg(gauge_color))
        .ratio(level as f64)
        .label("");

    f.render_widget(gauge, rms_layout[1]);

    if peak_level > 0.01 && rms_layout[1].width > 4 {
        let peak_pos = ((peak_level * (rms_layout[1].width - 4) as f32) as u16 + 1)
            .min(rms_layout[1].width - 2);
        let peak_area = Rect {
            x: rms_layout[1].x + peak_pos,
            y: rms_layout[1].y,
            width: 1,
            height: 1,
        };
        let peak_indicator = Paragraph::new("│").style(
            Style::default()
                .fg(Color::Rgb(255, 255, 255))
                .add_modifier(Modifier::BOLD),
        );
        f.render_widget(peak_indicator, peak_area);
    }
}

fn draw_eq_spectrum(f: &mut Frame, area: Rect, app: &App) {
    let mode_str = if app.linear_mode { "Linear" } else { "dB" };
    let title = format!(" EQ Spectrum ({}) ", mode_str);
    let eq_block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(96, 160, 192)));

    if let Some(ref spectrum) = app.last_spectrum {
        let inner = eq_block.inner(area);
        f.render_widget(eq_block, area);

        let max_bars = (inner.width as usize - 2) / 2;

        let bars: Vec<Bar> = (0..max_bars)
            .map(|i| {
                // Use logarithmic mapping to match frequency distribution
                let t = i as f32 / (max_bars - 1) as f32;
                let band_idx_f = t * (spectrum.bands.len() - 1) as f32;
                
                // Interpolate between adjacent bands for smoother display
                let band_idx_low = band_idx_f.floor() as usize;
                let band_idx_high = (band_idx_low + 1).min(spectrum.bands.len() - 1);
                let frac = band_idx_f - band_idx_low as f32;

                // Use appropriate data based on mode
                let (bands_data, _linear_data) = if app.linear_mode {
                    (&spectrum.bands_linear, &spectrum.bands_linear)
                } else {
                    (&spectrum.bands, &spectrum.bands_linear)
                };

                let level_low = bands_data[band_idx_low];
                let level_high = bands_data[band_idx_high];
                let level = level_low + frac * (level_high - level_low);

                let height = (level * 100.0) as u64;
                Bar::default()
                    .value(height)
                    .text_value(String::new())
                    .style(Style::default().fg(create_color_gradient(level)))
            })
            .collect();

        let barchart = BarChart::default()
            .block(Block::default())
            .data(BarGroup::default().bars(&bars))
            .bar_width(1)
            .bar_gap(1);

        f.render_widget(barchart, inner);
    } else {
        let waiting = Paragraph::new("Waiting for audio data...")
            .style(Style::default().fg(Color::Rgb(128, 128, 128)))
            .alignment(Alignment::Center)
            .block(eq_block);
        f.render_widget(waiting, area);
    }
}

fn draw_status_bar(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(96, 160, 192)));
    
    let inner = block.inner(area);
    f.render_widget(block, area);
    
    // Calculate content width to determine layout
    let device_text = format!("Device: {}", app.device_name);
    let sample_rate_text = format!("Sample Rate: {} Hz", app.sample_rate);
    let controls_text = "Controls: Q/ESC to quit, L to toggle Linear/dB";
    
    let total_content_width = device_text.len() + sample_rate_text.len() + controls_text.len() + 6; // Add separators
    let device_and_rate_width = device_text.len() + sample_rate_text.len() + 3; // Add separator
    
    let status_text = if total_content_width <= inner.width as usize {
        // Single line if everything fits
        vec![Line::from(vec![
            Span::styled("Device: ", Style::default().fg(Color::Rgb(128, 160, 192))),
            Span::styled(app.device_name.clone(), Style::default().fg(Color::White)),
            Span::styled(" | Sample Rate: ", Style::default().fg(Color::Rgb(128, 160, 192))),
            Span::styled(format!("{} Hz", app.sample_rate), Style::default().fg(Color::White)),
            Span::styled(" | Controls: ", Style::default().fg(Color::Rgb(128, 160, 192))),
            Span::styled("Q", Style::default().fg(Color::Rgb(255, 255, 0)).add_modifier(Modifier::BOLD)),
            Span::styled("/", Style::default().fg(Color::Rgb(128, 160, 192))),
            Span::styled("ESC", Style::default().fg(Color::Rgb(255, 255, 0)).add_modifier(Modifier::BOLD)),
            Span::styled(" to quit, ", Style::default().fg(Color::White)),
            Span::styled("L", Style::default().fg(Color::Rgb(255, 255, 0)).add_modifier(Modifier::BOLD)),
            Span::styled(" to toggle Linear/dB", Style::default().fg(Color::White)),
        ])]
    } else if device_and_rate_width <= inner.width as usize {
        // Two lines: device+sample rate on first line, controls on second
        vec![
            Line::from(vec![
                Span::styled("Device: ", Style::default().fg(Color::Rgb(128, 160, 192))),
                Span::styled(app.device_name.clone(), Style::default().fg(Color::White)),
                Span::styled(" | Sample Rate: ", Style::default().fg(Color::Rgb(128, 160, 192))),
                Span::styled(format!("{} Hz", app.sample_rate), Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("Controls: ", Style::default().fg(Color::Rgb(128, 160, 192))),
                Span::styled("Q", Style::default().fg(Color::Rgb(255, 255, 0)).add_modifier(Modifier::BOLD)),
                Span::styled("/", Style::default().fg(Color::Rgb(128, 160, 192))),
                Span::styled("ESC", Style::default().fg(Color::Rgb(255, 255, 0)).add_modifier(Modifier::BOLD)),
                Span::styled(" to quit, ", Style::default().fg(Color::White)),
                Span::styled("L", Style::default().fg(Color::Rgb(255, 255, 0)).add_modifier(Modifier::BOLD)),
                Span::styled(" to toggle Linear/dB", Style::default().fg(Color::White)),
            ])
        ]
    } else {
        // Three lines for very narrow terminals
        vec![
            Line::from(vec![
                Span::styled("Device: ", Style::default().fg(Color::Rgb(128, 160, 192))),
                Span::styled(app.device_name.clone(), Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("Sample Rate: ", Style::default().fg(Color::Rgb(128, 160, 192))),
                Span::styled(format!("{} Hz", app.sample_rate), Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("Q", Style::default().fg(Color::Rgb(255, 255, 0)).add_modifier(Modifier::BOLD)),
                Span::styled("/", Style::default().fg(Color::Rgb(128, 160, 192))),
                Span::styled("ESC", Style::default().fg(Color::Rgb(255, 255, 0)).add_modifier(Modifier::BOLD)),
                Span::styled(" quit, ", Style::default().fg(Color::White)),
                Span::styled("L", Style::default().fg(Color::Rgb(255, 255, 0)).add_modifier(Modifier::BOLD)),
                Span::styled(" toggle", Style::default().fg(Color::White)),
            ])
        ]
    };

    let status = Paragraph::new(status_text)
        .alignment(Alignment::Center);

    f.render_widget(status, inner);
}

fn draw_frequency_labels(f: &mut Frame, area: Rect, app: &App) {
    // Frequency range matches the FFT analysis (20 Hz to 20 kHz)
    let f_lo = 20.0f32;
    let f_hi = (app.sample_rate as f32 / 2.0).min(20_000.0);
    
    let label_block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_style(Style::default().fg(Color::Rgb(96, 160, 192)));
    
    let inner = label_block.inner(area);
    f.render_widget(label_block, area);
    
    // Split area for frequency values and label
    let freq_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Frequency values
            Constraint::Length(1), // "Frequency (Hz)" label
        ])
        .split(inner);
    
    // Calculate how many labels we can fit
    let label_spacing = 10; // Minimum characters between labels
    let max_labels = (inner.width as usize) / label_spacing;
    let num_labels = max_labels.min(5); // Limit to reasonable number
    
    if num_labels > 1 {
        let mut freq_positions = Vec::new();
        
        // Calculate positions and frequencies
        for i in 0..num_labels {
            let t = i as f32 / (num_labels - 1) as f32;
            let freq = f_lo * (f_hi / f_lo).powf(t);
            let pos = (t * (freq_layout[0].width - 1) as f32) as u16;
            
            let freq_str = if freq >= 1000.0 {
                format!("{:.0}k", freq / 1000.0)
            } else {
                format!("{:.0}", freq)
            };
            
            freq_positions.push((pos, freq_str));
        }
        
        // Render frequency values at calculated positions
        for (pos, freq_str) in freq_positions {
            let label_area = Rect {
                x: freq_layout[0].x + pos.min(freq_layout[0].width.saturating_sub(freq_str.len() as u16)),
                y: freq_layout[0].y,
                width: freq_str.len() as u16,
                height: 1,
            };
            
            let freq_label = Paragraph::new(freq_str)
                .style(Style::default().fg(Color::Rgb(160, 160, 160)));
            f.render_widget(freq_label, label_area);
        }
    }
    
    // Add "Frequency (Hz)" subtitle
    let subtitle = Paragraph::new("Frequency (Hz)")
        .style(Style::default().fg(Color::Rgb(128, 128, 128)))
        .alignment(Alignment::Center);
    f.render_widget(subtitle, freq_layout[1]);
}
