use ratatui::{
    layout::Rect,
    prelude::{Color, Frame, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};

use crate::app::{App, MAX_LISTED_SATELLITES};

use super::truncate;

pub(super) fn render_panel(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut lines = vec![
        styled_line("Mission", Color::White),
        styled_line("1. Position the ship", Color::Gray),
        styled_line("2. Power the scanner", Color::Gray),
        styled_line("3. Lock a target", Color::Gray),
        Line::from(""),
        styled_line(format!("lat {:>8.3}", app.selected_lat), Color::White),
        styled_line(format!("lon {:>8.3}", app.selected_lon), Color::White),
        styled_line(
            app.observer_altitude_m
                .map(|alt| format!("alt {:>8.1} m", alt))
                .unwrap_or_else(|| "alt       -- m".to_owned()),
            Color::White,
        ),
        styled_line(
            format!("scanner {:>3} deg", app.search_radius),
            Color::White,
        ),
        Line::from(""),
        styled_line("Orbital contacts", Color::Cyan),
    ];

    if app.satellites.is_empty() {
        lines.push(styled_line("no signals yet", Color::DarkGray));
    } else {
        for (index, sat) in app
            .satellites
            .iter()
            .take(MAX_LISTED_SATELLITES)
            .enumerate()
        {
            let name = truncate(&sat.name, 20);
            let selected = app.selected_satellite_index == Some(index);
            let marker = if selected { ">" } else { " " };
            let color = if selected {
                Color::LightMagenta
            } else {
                Color::Cyan
            };
            lines.push(styled_line(
                format!("{marker} {} {:>5.0}km", name, sat.altitude_km),
                color,
            ));
            lines.push(styled_line(
                format!("  #{} {:>6.1},{:>6.1}", sat.id, sat.lat, sat.lon),
                Color::DarkGray,
            ));
        }
    }

    let panel = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(Span::styled(
                    " REONA / SPACE HUD ",
                    Style::default()
                        .fg(Color::LightMagenta)
                        .add_modifier(Modifier::BOLD),
                )),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(panel, area);
}

pub(super) fn render_status(
    frame: &mut Frame<'_>,
    area: Rect,
    status_height: u16,
    app: &App,
    compact: bool,
) {
    let color = if app.loading {
        Color::Yellow
    } else {
        Color::White
    };
    let status_area = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(status_height),
        width: area.width,
        height: status_height,
    };

    if status_height == 1 {
        let status = format!(
            "{} | Arrows pilot | Enter scan | Tab lock | t track | q quit",
            app.message
        );
        frame.render_widget(
            Paragraph::new(status).style(Style::default().fg(color)),
            status_area,
        );
        return;
    }

    let location = if compact {
        format!("ship {:.2},{:.2}", app.selected_lat, app.selected_lon)
    } else if let Some(sat) = app.selected_satellite() {
        format!("target {} #{}", truncate(&sat.name, 22), sat.id)
    } else {
        format!("ship {:.2},{:.2}", app.selected_lat, app.selected_lon)
    };

    let hint_line = if app.tracking {
        Line::from(vec![
            keycap("t"),
            Span::raw(" HUD  "),
            keycap("r"),
            Span::raw(" pulse  "),
            keycap("q"),
            Span::raw(" quit"),
        ])
    } else {
        Line::from(vec![
            keycap("Arrows"),
            Span::raw(" pilot  "),
            keycap("Click"),
            Span::raw(" land  "),
            keycap("Enter"),
            Span::raw(" scan  "),
            keycap("Tab"),
            Span::raw(" lock  "),
            keycap("t"),
            Span::raw(" track  "),
            keycap("+/-"),
            Span::raw(" scanner  "),
            keycap("q"),
            Span::raw(" quit"),
        ])
    };

    let status = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(
                " HUD ",
                Style::default()
                    .fg(Color::Black)
                    .bg(color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(app.message.clone(), Style::default().fg(color)),
            Span::styled(
                format!("  {location}"),
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        hint_line,
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::DarkGray)),
    );

    frame.render_widget(status, status_area);
}

fn keycap(content: &'static str) -> Span<'static> {
    Span::styled(
        format!(" {content} "),
        Style::default()
            .fg(Color::Black)
            .bg(Color::LightMagenta)
            .add_modifier(Modifier::BOLD),
    )
}

fn styled_line<'a>(content: impl Into<std::borrow::Cow<'a, str>>, color: Color) -> Line<'a> {
    Line::from(Span::styled(content, Style::default().fg(color)))
}
