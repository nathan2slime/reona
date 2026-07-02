use std::f64::consts::PI;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    prelude::{Color, Frame, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use crate::app::{App, normalize_lon};

mod continents;
mod panel;

use continents::CONTINENTS;

#[derive(Clone)]
struct MapPoint {
    name: String,
    lat: f64,
    lon: f64,
    symbol: char,
    color: Color,
    selected: bool,
}

#[derive(Clone, Copy)]
struct Cell {
    ch: char,
    color: Color,
}

#[derive(Clone, Copy, Default)]
pub struct GlobeGeometry {
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
}

pub fn render(frame: &mut Frame<'_>, app: &App, rotation: f64) -> GlobeGeometry {
    let area = frame.area();
    frame.render_widget(Clear, area);

    let status_height = if area.height >= 9 { 4 } else { 1 };
    let content_height = area.height.saturating_sub(status_height);
    let panel_width = if !app.tracking && area.width >= 74 {
        (area.width / 3).min(38)
    } else {
        0
    };
    let panel_gap = if panel_width > 0 { 1 } else { 0 };
    let globe_width = area.width.saturating_sub(panel_width + panel_gap);
    let globe_area = Rect {
        x: area.x,
        y: area.y,
        width: globe_width,
        height: content_height,
    };
    let geometry = geometry_for(globe_area);

    draw_starfield(frame.buffer_mut(), globe_area, geometry, rotation);
    draw_outline(frame.buffer_mut(), globe_area, geometry);

    if app.tracking {
        draw_continents(frame.buffer_mut(), globe_area, geometry, rotation);
        draw_tracking(frame.buffer_mut(), globe_area, geometry, rotation, app);
        render_tracking_title(frame, globe_area, app);
    } else {
        draw_grid(frame.buffer_mut(), globe_area, geometry, rotation);
        draw_continents(frame.buffer_mut(), globe_area, geometry, rotation);
        draw_sensor_radius(frame.buffer_mut(), globe_area, geometry, rotation, app);
        draw_points(
            frame.buffer_mut(),
            globe_area,
            geometry,
            rotation,
            &points(app),
        );
    }

    if panel_width > 0 {
        let panel_area = Rect {
            x: globe_area.x + globe_area.width + panel_gap,
            y: area.y,
            width: panel_width,
            height: content_height,
        };
        panel::render_panel(frame, panel_area, app);
    }

    panel::render_status(frame, area, status_height, app, panel_width == 0);
    geometry
}

pub fn screen_to_lat_lon(
    screen_x: f64,
    screen_y: f64,
    geometry: GlobeGeometry,
    rotation: f64,
) -> Option<(f64, f64)> {
    let x = (screen_x - geometry.cx) / geometry.rx;
    let y = (geometry.cy - screen_y) / geometry.ry;

    if x * x + y * y > 1.0 {
        return None;
    }

    let z = (1.0 - x * x - y * y).sqrt();
    let lat = y.asin().to_degrees();
    let lon = normalize_lon((x.atan2(z) - rotation).to_degrees());
    Some((lat, lon))
}

fn geometry_for(area: Rect) -> GlobeGeometry {
    let cx = area.x as f64 + area.width as f64 / 2.0;
    let cy = area.y as f64 + area.height as f64 / 2.0;
    let max_rx = (area.width as f64 * 0.44).max(4.0);
    let max_ry = (area.height as f64 * 0.36).max(4.0);
    let rx = (max_ry * 2.0).min(max_rx);
    let ry = (rx / 2.0).min(max_ry);

    GlobeGeometry { cx, cy, rx, ry }
}

fn points(app: &App) -> Vec<MapPoint> {
    let mut points = Vec::with_capacity(app.satellites.len() + 1);
    points.push(MapPoint {
        name: "ship".to_owned(),
        lat: app.selected_lat,
        lon: app.selected_lon,
        symbol: '^',
        color: Color::Yellow,
        selected: false,
    });

    for (index, sat) in app.satellites.iter().enumerate() {
        let selected = app.selected_satellite_index == Some(index);
        points.push(MapPoint {
            name: sat.name.clone(),
            lat: sat.lat,
            lon: sat.lon,
            symbol: if selected { '*' } else { '+' },
            color: if selected {
                Color::LightMagenta
            } else {
                Color::Cyan
            },
            selected,
        });
    }

    points
}

fn draw_starfield(buffer: &mut Buffer, area: Rect, geometry: GlobeGeometry, rotation: f64) {
    let phase = (rotation * 12.0) as u32;
    let max_y = area.y.saturating_add(area.height);
    let max_x = area.x.saturating_add(area.width);

    for y in area.y..max_y {
        for x in area.x..max_x {
            let nx = (x as f64 - geometry.cx) / geometry.rx.max(1.0);
            let ny = (y as f64 - geometry.cy) / geometry.ry.max(1.0);
            if nx * nx + ny * ny <= 1.08 {
                continue;
            }

            let hash = (x as u32)
                .wrapping_mul(37)
                .wrapping_add((y as u32).wrapping_mul(91))
                .wrapping_add(phase.wrapping_mul(13));

            if hash.is_multiple_of(43) {
                let ch = if hash.is_multiple_of(5) { '*' } else { '.' };
                let color = if hash.is_multiple_of(7) {
                    Color::Cyan
                } else {
                    Color::DarkGray
                };
                buffer[(x, y)]
                    .set_symbol(&ch.to_string())
                    .set_style(Style::default().fg(color));
            }
        }
    }
}

fn draw_sensor_radius(
    buffer: &mut Buffer,
    area: Rect,
    geometry: GlobeGeometry,
    rotation: f64,
    app: &App,
) {
    if app.search_radius == 0 {
        return;
    }

    let origin_lat = app.selected_lat.to_radians();
    let origin_lon = app.selected_lon.to_radians();
    let scan_radius = (app.search_radius as f64).to_radians();

    for bearing_deg in (0..360).step_by(4) {
        let bearing = (bearing_deg as f64).to_radians();
        let lat = (origin_lat.sin() * scan_radius.cos()
            + origin_lat.cos() * scan_radius.sin() * bearing.cos())
        .asin();
        let lon = origin_lon
            + (bearing.sin() * scan_radius.sin() * origin_lat.cos())
                .atan2(scan_radius.cos() - origin_lat.sin() * lat.sin());

        if let Some((x, y, _z)) =
            project(lat.to_degrees(), normalize_lon(lon.to_degrees()), rotation)
        {
            put(
                buffer,
                area,
                geometry.cx + x * geometry.rx,
                geometry.cy - y * geometry.ry,
                Cell {
                    ch: '~',
                    color: Color::LightBlue,
                },
            );
        }
    }
}

fn draw_grid(buffer: &mut Buffer, area: Rect, geometry: GlobeGeometry, rotation: f64) {
    for lat in (-60..=60).step_by(30) {
        for lon in (0..360).step_by(3) {
            if let Some((x, y, z)) = project(lat as f64, lon as f64, rotation) {
                put(
                    buffer,
                    area,
                    geometry.cx + x * geometry.rx,
                    geometry.cy - y * geometry.ry,
                    Cell {
                        ch: shade(z),
                        color: Color::DarkGray,
                    },
                );
            }
        }
    }

    for lon in (0..360).step_by(30) {
        for lat in (-90..=90).step_by(3) {
            if let Some((x, y, z)) = project(lat as f64, lon as f64, rotation) {
                put(
                    buffer,
                    area,
                    geometry.cx + x * geometry.rx,
                    geometry.cy - y * geometry.ry,
                    Cell {
                        ch: shade(z),
                        color: Color::DarkGray,
                    },
                );
            }
        }
    }
}

fn draw_points(
    buffer: &mut Buffer,
    area: Rect,
    geometry: GlobeGeometry,
    rotation: f64,
    points: &[MapPoint],
) {
    for point in points {
        if let Some((x, y, _z)) = project(point.lat, point.lon, rotation) {
            let sx = geometry.cx + x * geometry.rx;
            let sy = geometry.cy - y * geometry.ry;
            put(
                buffer,
                area,
                sx,
                sy,
                Cell {
                    ch: point.symbol,
                    color: point.color,
                },
            );

            if point.symbol == '^' || point.selected || points.len() <= 6 {
                draw_label(buffer, area, sx + 2.0, sy, &point.name, point.color);
            }
        }
    }
}

fn draw_tracking(
    buffer: &mut Buffer,
    area: Rect,
    geometry: GlobeGeometry,
    rotation: f64,
    app: &App,
) {
    for position in &app.tracking_positions {
        if let Some((x, y, _z)) = project(position.lat, position.lon, rotation) {
            put(
                buffer,
                area,
                geometry.cx + x * geometry.rx,
                geometry.cy - y * geometry.ry,
                Cell {
                    ch: '.',
                    color: Color::DarkGray,
                },
            );
        }
    }

    let Some(position) = app.current_tracking_position().or_else(|| {
        app.selected_satellite()
            .map(|sat| crate::client::SatellitePosition {
                lat: sat.lat,
                lon: sat.lon,
                altitude_km: sat.altitude_km,
                timestamp: 0,
            })
    }) else {
        return;
    };

    let Some((x, y, _z)) = project(position.lat, position.lon, rotation) else {
        return;
    };

    let sx = geometry.cx + x * geometry.rx;
    let sy = geometry.cy - y * geometry.ry;
    put(
        buffer,
        area,
        sx,
        sy,
        Cell {
            ch: '*',
            color: Color::LightMagenta,
        },
    );
    if let Some(sat) = app.selected_satellite() {
        draw_label(buffer, area, sx + 2.0, sy, &sat.name, Color::LightMagenta);
    }
}

fn render_tracking_title(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let Some(sat) = app.selected_satellite() else {
        return;
    };
    let lat = app
        .current_tracking_position()
        .map(|position| (position.lat, position.lon, position.altitude_km))
        .unwrap_or((sat.lat, sat.lon, sat.altitude_km));
    let title_area = Rect {
        x: area.x.saturating_add(1),
        y: area.y,
        width: area.width.min(56),
        height: 3.min(area.height),
    };
    let title = Paragraph::new(vec![Line::from(vec![
        Span::styled(
            " ORBIT LOCK ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::LightMagenta)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            format!("{} #{}", truncate(&sat.name, 28), sat.id),
            Style::default().fg(Color::White),
        ),
        Span::styled(
            format!("  {:.1},{:.1}  {:.0}km", lat.0, lat.1, lat.2),
            Style::default().fg(Color::DarkGray),
        ),
    ])])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(title, title_area);
}

fn draw_continents(buffer: &mut Buffer, area: Rect, geometry: GlobeGeometry, rotation: f64) {
    for continent in CONTINENTS {
        draw_geo_polyline(
            buffer,
            area,
            geometry,
            rotation,
            continent,
            Cell {
                ch: '▒',
                color: Color::Blue,
            },
        );
    }
}

fn draw_geo_polyline(
    buffer: &mut Buffer,
    area: Rect,
    geometry: GlobeGeometry,
    rotation: f64,
    points: &[(f64, f64)],
    cell: Cell,
) {
    for segment in points.windows(2) {
        let (from_lat, from_lon) = segment[0];
        let (to_lat, to_lon) = segment[1];
        let lat_delta = to_lat - from_lat;
        let lon_delta = to_lon - from_lon;

        if lon_delta.abs() > 180.0 {
            continue;
        }

        let steps = ((lat_delta.abs().max(lon_delta.abs()) / 2.0).ceil() as usize).max(1);
        for step in 0..=steps {
            let t = step as f64 / steps as f64;
            let lat = from_lat + lat_delta * t;
            let lon = from_lon + lon_delta * t;
            if let Some((x, y, _z)) = project(lat, lon, rotation) {
                put(
                    buffer,
                    area,
                    geometry.cx + x * geometry.rx,
                    geometry.cy - y * geometry.ry,
                    cell,
                );
            }
        }
    }
}

fn project(lat_deg: f64, lon_deg: f64, rotation: f64) -> Option<(f64, f64, f64)> {
    let lat = lat_deg.to_radians();
    let lon = lon_deg.to_radians() + rotation;
    let x = lat.cos() * lon.sin();
    let y = lat.sin();
    let z = lat.cos() * lon.cos();

    if z > 0.0 { Some((x, y, z)) } else { None }
}

fn shade(z: f64) -> char {
    if z > 0.85 {
        '•'
    } else if z > 0.55 {
        '·'
    } else {
        '.'
    }
}

fn draw_outline(buffer: &mut Buffer, area: Rect, geometry: GlobeGeometry) {
    for i in 0..360 {
        let a = i as f64 * PI / 180.0;
        let x = geometry.cx + a.cos() * geometry.rx;
        let y = geometry.cy + a.sin() * geometry.ry;
        put(
            buffer,
            area,
            x,
            y,
            Cell {
                ch: '#',
                color: Color::DarkGray,
            },
        );
    }
}

fn draw_label(buffer: &mut Buffer, area: Rect, x: f64, y: f64, text: &str, color: Color) {
    let py = y.round() as i32;

    for (px, ch) in (x.round() as i32..).zip(text.chars().take(24)) {
        put(buffer, area, px as f64, py as f64, Cell { ch, color });
    }
}

fn truncate(text: &str, max_chars: usize) -> String {
    let mut value: String = text.chars().take(max_chars).collect();
    if text.chars().count() > max_chars {
        value.push('~');
    }
    value
}

fn put(buffer: &mut Buffer, area: Rect, x: f64, y: f64, cell: Cell) {
    let xi = x.round() as i32;
    let yi = y.round() as i32;

    if xi < area.x as i32 || yi < area.y as i32 {
        return;
    }

    let xi = xi as u16;
    let yi = yi as u16;

    if xi >= area.x + area.width || yi >= area.y + area.height {
        return;
    }

    buffer[(xi, yi)]
        .set_symbol(&cell.ch.to_string())
        .set_style(Style::default().fg(cell.color));
}
