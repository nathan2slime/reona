mod app;
mod client;
mod config;
mod ui;

use std::{
    io::stdout,
    time::{Duration, Instant},
};

use app::App;
use client::SatelliteDataClient;
use config::env::AppConfig;
use crossterm::{
    cursor::{Hide, Show},
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseButton, MouseEventKind,
    },
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{Terminal, backend::CrosstermBackend};

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
}

impl TerminalGuard {
    fn enter() -> std::io::Result<Self> {
        terminal::enable_raw_mode()?;

        let mut out = stdout();
        execute!(out, EnterAlternateScreen, Hide, EnableMouseCapture)?;

        let backend = CrosstermBackend::new(out);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        Ok(Self { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(
            self.terminal.backend_mut(),
            Show,
            DisableMouseCapture,
            LeaveAlternateScreen
        );
        let _ = terminal::disable_raw_mode();
    }
}

fn main() -> std::io::Result<()> {
    let config = AppConfig::from_env().map_err(|error| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, error.to_string())
    })?;
    let api = SatelliteDataClient::new(&config).map_err(std::io::Error::other)?;
    let mut app = App::new(api, config);
    let mut terminal = TerminalGuard::enter()?;
    let start = Instant::now();

    loop {
        app.refresh_tracking_if_needed();
        let elapsed = start.elapsed().as_secs_f64();
        let rotation = elapsed * 0.25;
        let render_rotation = if app.tracking {
            tracking_rotation(&app).unwrap_or(rotation)
        } else {
            rotation
        };
        let mut geometry = ui::GlobeGeometry::default();
        terminal.terminal.draw(|frame| {
            geometry = ui::render(frame, &app, render_rotation);
        })?;

        if event::poll(Duration::from_millis(33))? {
            match event::read()? {
                Event::Key(key) => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('r') => app.refresh_tracking_now(),
                    KeyCode::Char('t') => app.toggle_tracking(),
                    KeyCode::Tab => app.select_next_satellite(),
                    KeyCode::Char('c') => {
                        app.clear_results();
                        app.message =
                            "Mission 1: choose another orbital zone and press Enter.".to_owned();
                    }
                    KeyCode::Enter if !app.tracking => app.fetch(),
                    KeyCode::Up if !app.tracking => app.move_selection(2.0, 0.0),
                    KeyCode::Down if !app.tracking => app.move_selection(-2.0, 0.0),
                    KeyCode::Left if !app.tracking => app.move_selection(0.0, -4.0),
                    KeyCode::Right if !app.tracking => app.move_selection(0.0, 4.0),
                    KeyCode::Char('+') | KeyCode::Char('=') => {
                        app.search_radius = app.search_radius.saturating_add(5).min(90);
                        app.message =
                            format!("Scanner range adjusted to {} degrees.", app.search_radius);
                    }
                    KeyCode::Char('-') => {
                        app.search_radius = app.search_radius.saturating_sub(5);
                        app.message =
                            format!("Scanner range adjusted to {} degrees.", app.search_radius);
                    }
                    _ => {}
                },
                Event::Mouse(mouse) => {
                    if !app.tracking
                        && matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
                        && let Some((lat, lon)) = ui::screen_to_lat_lon(
                            mouse.column as f64,
                            mouse.row as f64,
                            geometry,
                            render_rotation,
                        )
                    {
                        app.set_selection(lat, lon);
                    }
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn tracking_rotation(app: &App) -> Option<f64> {
    let lon = app
        .current_tracking_position()
        .map(|position| position.lon)
        .or_else(|| app.selected_satellite().map(|satellite| satellite.lon))?;

    Some(-lon.to_radians())
}
