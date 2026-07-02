use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    client::{SatelliteDataClient, SatellitePosition},
    config::env::AppConfig,
};

pub const MAX_LISTED_SATELLITES: usize = 10;
const TRACKING_WINDOW_SECONDS: u16 = 40;
const TRACKING_REFRESH_LEAD_SECONDS: u64 = 5;
const TRACKING_RETRY_SECONDS: u64 = 5;

pub struct App {
    api: SatelliteDataClient,
    pub selected_lat: f64,
    pub selected_lon: f64,
    pub observer_altitude_m: Option<f64>,
    pub satellites: Vec<crate::client::Satellite>,
    pub message: String,
    pub loading: bool,
    pub search_radius: u8,
    pub category_id: u32,
    pub selected_satellite_index: Option<usize>,
    pub tracking: bool,
    pub tracking_positions: Vec<SatellitePosition>,
    tracking_satellite_id: Option<u32>,
    tracking_retry_after_epoch: u64,
}

impl App {
    pub fn new(api: SatelliteDataClient, config: AppConfig) -> Self {
        Self {
            api,
            selected_lat: -23.5505,
            selected_lon: -46.6333,
            observer_altitude_m: None,
            satellites: Vec::new(),
            message: "Mission 1: position the ship and press Enter to scan orbit.".to_owned(),
            loading: false,
            search_radius: config.default_search_radius,
            category_id: config.default_category_id,
            selected_satellite_index: None,
            tracking: false,
            tracking_positions: Vec::new(),
            tracking_satellite_id: None,
            tracking_retry_after_epoch: 0,
        }
    }

    pub fn move_selection(&mut self, lat_delta: f64, lon_delta: f64) {
        self.selected_lat = (self.selected_lat + lat_delta).clamp(-90.0, 90.0);
        self.selected_lon = normalize_lon(self.selected_lon + lon_delta);
        self.clear_results();
        self.message = "Ship repositioned. Press Enter to activate the orbital scanner.".to_owned();
    }

    pub fn set_selection(&mut self, lat: f64, lon: f64) {
        self.selected_lat = lat.clamp(-90.0, 90.0);
        self.selected_lon = normalize_lon(lon);
        self.clear_results();
        self.message = "Landing zone marked. Press Enter to activate the scanner.".to_owned();
    }

    pub fn clear_results(&mut self) {
        self.observer_altitude_m = None;
        self.satellites.clear();
        self.selected_satellite_index = None;
        self.tracking = false;
        self.tracking_positions.clear();
        self.tracking_satellite_id = None;
        self.tracking_retry_after_epoch = 0;
    }

    pub fn select_next_satellite(&mut self) {
        if self.satellites.is_empty() {
            self.selected_satellite_index = None;
            return;
        }

        self.selected_satellite_index = Some(
            self.selected_satellite_index
                .map(|index| (index + 1) % self.satellites.len())
                .unwrap_or(0),
        );
        self.tracking_positions.clear();
        self.tracking_satellite_id = None;
        self.message = "Target locked. Press t to track the orbit on a clean display.".to_owned();
    }

    pub fn toggle_tracking(&mut self) {
        if self.satellites.is_empty() {
            self.message = "Activate the scanner before starting tracking.".to_owned();
            return;
        }

        if self.selected_satellite_index.is_none() {
            self.selected_satellite_index = Some(0);
        }

        self.tracking = !self.tracking;
        self.message = if self.tracking {
            "Orbital tracking mode active. Press t to return to the HUD.".to_owned()
        } else {
            "Mission HUD reactivated.".to_owned()
        };

        if self.tracking {
            self.refresh_tracking_positions(true);
        } else {
            self.tracking_positions.clear();
            self.tracking_satellite_id = None;
        }
    }

    pub fn selected_satellite(&self) -> Option<&crate::client::Satellite> {
        self.selected_satellite_index
            .and_then(|index| self.satellites.get(index))
    }

    pub fn fetch(&mut self) {
        self.loading = true;
        self.message = "Mission 2: scanning altitude and orbital contacts via N2YO...".to_owned();
        let previous_satellite_id = self.selected_satellite().map(|satellite| satellite.id);
        let was_tracking = self.tracking;

        match self.api.satellites_above(
            self.selected_lat,
            self.selected_lon,
            self.search_radius,
            self.category_id,
        ) {
            Ok(mut search) => {
                let upstream_count = search.satellites.len();
                search.satellites.truncate(MAX_LISTED_SATELLITES);
                let shown_count = search.satellites.len();
                self.observer_altitude_m = Some(search.observer_altitude_m);
                self.satellites = search.satellites;
                self.selected_satellite_index = previous_satellite_id
                    .and_then(|id| {
                        self.satellites
                            .iter()
                            .position(|satellite| satellite.id == id)
                    })
                    .or_else(|| (!self.satellites.is_empty()).then_some(0));
                self.tracking = was_tracking && self.selected_satellite_index.is_some();
                if !self.tracking {
                    self.tracking_positions.clear();
                    self.tracking_satellite_id = None;
                }
                self.message = if self.tracking {
                    format!("Tracking refreshed. {shown_count}/{upstream_count} contacts on radar.")
                } else {
                    format!(
                        "Mission 3: {shown_count}/{upstream_count} contacts detected. Use Tab to lock, t to track."
                    )
                };
            }
            Err(error) => {
                self.clear_results();
                self.message = format!("Scanner failed: {error}. Check N2YO/Open-Meteo config.");
            }
        }

        self.loading = false;
    }

    pub fn refresh_tracking_if_needed(&mut self) {
        if !self.tracking {
            return;
        }

        let now = current_epoch_seconds();
        if now < self.tracking_retry_after_epoch {
            return;
        }

        let selected_id = self.selected_satellite().map(|satellite| satellite.id);
        let stale_target = self.tracking_satellite_id != selected_id;
        let stale_window = self
            .tracking_positions
            .last()
            .map(|position| position.timestamp <= now + TRACKING_REFRESH_LEAD_SECONDS)
            .unwrap_or(true);

        if stale_target || stale_window {
            self.refresh_tracking_positions(false);
        }
    }

    pub fn refresh_tracking_now(&mut self) {
        if self.tracking {
            self.refresh_tracking_positions(true);
        } else {
            self.fetch();
        }
    }

    pub fn current_tracking_position(&self) -> Option<SatellitePosition> {
        let now = current_epoch_seconds();
        interpolate_position(&self.tracking_positions, now)
    }

    fn refresh_tracking_positions(&mut self, force_message: bool) {
        let Some(satellite) = self.selected_satellite() else {
            self.tracking = false;
            self.message = "Select a satellite before starting tracking.".to_owned();
            return;
        };

        let sat_id = satellite.id;
        let sat_name = satellite.name.clone();
        let observer_altitude_m = self.observer_altitude_m.unwrap_or(0.0);
        self.loading = true;

        match self.api.satellite_positions(
            sat_id,
            self.selected_lat,
            self.selected_lon,
            observer_altitude_m,
            TRACKING_WINDOW_SECONDS,
        ) {
            Ok(positions) if !positions.is_empty() => {
                let count = positions.len();
                self.tracking_positions = positions;
                self.tracking_satellite_id = Some(sat_id);
                self.tracking_retry_after_epoch = 0;
                if force_message {
                    self.message = format!(
                        "Tracking {sat_name}. Loaded {count} future positions for the next {TRACKING_WINDOW_SECONDS}s."
                    );
                }
            }
            Ok(_) => {
                self.tracking_retry_after_epoch = current_epoch_seconds() + TRACKING_RETRY_SECONDS;
                self.message =
                    format!("Tracking feed for {sat_name} returned no positions. Retrying soon.");
            }
            Err(error) => {
                self.tracking_retry_after_epoch = current_epoch_seconds() + TRACKING_RETRY_SECONDS;
                self.message =
                    format!("Tracking feed failed for {sat_name}: {error}. Retrying soon.");
            }
        }

        self.loading = false;
    }
}

pub fn normalize_lon(lon: f64) -> f64 {
    (lon + 180.0).rem_euclid(360.0) - 180.0
}

fn current_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn interpolate_position(positions: &[SatellitePosition], now: u64) -> Option<SatellitePosition> {
    let first = positions.first()?;
    if now <= first.timestamp {
        return Some(first.clone());
    }

    for pair in positions.windows(2) {
        let from = &pair[0];
        let to = &pair[1];
        if now >= from.timestamp && now <= to.timestamp {
            let span = (to.timestamp - from.timestamp).max(1) as f64;
            let t = (now - from.timestamp) as f64 / span;
            let lon_delta = normalize_lon(to.lon - from.lon);
            return Some(SatellitePosition {
                lat: from.lat + (to.lat - from.lat) * t,
                lon: normalize_lon(from.lon + lon_delta * t),
                altitude_km: from.altitude_km + (to.altitude_km - from.altitude_km) * t,
                timestamp: now,
            });
        }
    }

    positions.last().cloned()
}
