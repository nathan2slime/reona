use std::{error::Error, fmt, time::Duration};

use reqwest::StatusCode;
use serde::{Deserialize, de::DeserializeOwned};

use crate::config::env::AppConfig;

#[derive(Clone)]
pub struct SatelliteDataClient {
    n2yo: N2yoClient,
    open_meteo: OpenMeteoClient,
}

impl SatelliteDataClient {
    pub fn new(config: &AppConfig) -> Result<Self, String> {
        Ok(Self {
            n2yo: N2yoClient::new(
                config.n2yo_base_url.clone(),
                config.n2yo_api_key.clone(),
                config.n2yo_timeout_seconds,
            )?,
            open_meteo: OpenMeteoClient::new(
                config.open_meteo_elevation_url.clone(),
                config.open_meteo_timeout_seconds,
            )?,
        })
    }

    pub fn satellites_above(
        &self,
        lat: f64,
        lon: f64,
        search_radius: u8,
        category_id: u32,
    ) -> Result<SatelliteSearch, SatelliteDataError> {
        validate_observer(lat, lon)?;

        if search_radius > 90 {
            return Err(SatelliteDataError::Validation(
                "search radius must be between 0 and 90".to_owned(),
            ));
        }

        let elevation = self.open_meteo.elevation(lat, lon)?;
        let above = self
            .n2yo
            .above(lat, lon, elevation.altitude_m, search_radius, category_id)?;

        let satellites = above
            .above
            .into_iter()
            .map(|sat| Satellite {
                id: sat.satid,
                name: sat.satname,
                lat: sat.satlat,
                lon: sat.satlng,
                altitude_km: sat.satalt,
            })
            .collect();

        Ok(SatelliteSearch {
            observer_altitude_m: elevation.altitude_m,
            satellites,
        })
    }

    pub fn satellite_positions(
        &self,
        sat_id: u32,
        observer_lat: f64,
        observer_lon: f64,
        observer_altitude_m: f64,
        seconds: u16,
    ) -> Result<Vec<SatellitePosition>, SatelliteDataError> {
        validate_observer(observer_lat, observer_lon)?;

        if seconds == 0 || seconds > 300 {
            return Err(SatelliteDataError::Validation(
                "seconds must be between 1 and 300".to_owned(),
            ));
        }

        let response = self.n2yo.positions(
            sat_id,
            observer_lat,
            observer_lon,
            observer_altitude_m,
            seconds,
        )?;

        response
            .positions
            .into_iter()
            .map(|position| {
                let timestamp = position.timestamp.try_into().map_err(|_| {
                    SatelliteDataError::InvalidResponse(
                        "N2YO position timestamp was negative".to_owned(),
                    )
                })?;

                Ok(SatellitePosition {
                    lat: position.satlatitude,
                    lon: position.satlongitude,
                    altitude_km: position.sataltitude,
                    timestamp,
                })
            })
            .collect()
    }
}

#[derive(Debug)]
pub enum SatelliteDataError {
    N2yo(N2yoClientError),
    OpenMeteo(OpenMeteoClientError),
    InvalidResponse(String),
    Validation(String),
}

impl fmt::Display for SatelliteDataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::N2yo(error) => write!(f, "{error}"),
            Self::OpenMeteo(error) => write!(f, "{error}"),
            Self::InvalidResponse(message) => write!(f, "invalid satellite data: {message}"),
            Self::Validation(message) => write!(f, "invalid scanner request: {message}"),
        }
    }
}

impl Error for SatelliteDataError {}

impl From<N2yoClientError> for SatelliteDataError {
    fn from(error: N2yoClientError) -> Self {
        Self::N2yo(error)
    }
}

impl From<OpenMeteoClientError> for SatelliteDataError {
    fn from(error: OpenMeteoClientError) -> Self {
        Self::OpenMeteo(error)
    }
}

pub struct SatelliteSearch {
    pub observer_altitude_m: f64,
    pub satellites: Vec<Satellite>,
}

#[derive(Clone)]
pub struct Satellite {
    pub id: u32,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub altitude_km: f64,
}

#[derive(Clone)]
pub struct SatellitePosition {
    pub lat: f64,
    pub lon: f64,
    pub altitude_km: f64,
    pub timestamp: u64,
}

#[derive(Clone)]
struct N2yoClient {
    http: reqwest::blocking::Client,
    base_url: String,
    api_key: String,
}

impl N2yoClient {
    fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        timeout_seconds: u64,
    ) -> Result<Self, String> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(timeout_seconds))
            .build()
            .map_err(|error| format!("failed to build N2YO HTTP client: {error}"))?;

        Ok(Self {
            http,
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            api_key: api_key.into(),
        })
    }

    fn positions(
        &self,
        id: u32,
        observer_lat: f64,
        observer_lng: f64,
        observer_alt: f64,
        seconds: u16,
    ) -> Result<PositionsResponse, N2yoClientError> {
        self.get_json(&format!(
            "positions/{id}/{observer_lat}/{observer_lng}/{observer_alt}/{seconds}/"
        ))
    }

    fn above(
        &self,
        observer_lat: f64,
        observer_lng: f64,
        observer_alt: f64,
        search_radius: u8,
        category_id: u32,
    ) -> Result<AboveResponse, N2yoClientError> {
        self.get_json(&format!(
            "above/{observer_lat}/{observer_lng}/{observer_alt}/{search_radius}/{category_id}/"
        ))
    }

    fn get_json<T>(&self, path: &str) -> Result<T, N2yoClientError>
    where
        T: DeserializeOwned,
    {
        let url = format!("{}/{}&apiKey={}", self.base_url, path, self.api_key);
        let response = self
            .http
            .get(url)
            .send()
            .map_err(|error| N2yoClientError::RequestFailed(error.to_string()))?;

        let status = response.status();
        let body = response
            .text()
            .map_err(|error| N2yoClientError::RequestFailed(error.to_string()))?;

        if !status.is_success() {
            return Err(N2yoClientError::UpstreamStatus { status, body });
        }

        serde_json::from_str(&body).map_err(|error| N2yoClientError::InvalidResponse {
            message: error.to_string(),
            body,
        })
    }
}

#[derive(Debug)]
pub enum N2yoClientError {
    RequestFailed(String),
    UpstreamStatus { status: StatusCode, body: String },
    InvalidResponse { message: String, body: String },
}

impl fmt::Display for N2yoClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RequestFailed(message) => write!(f, "N2YO request failed: {message}"),
            Self::UpstreamStatus { status, body } => {
                write!(f, "N2YO returned HTTP {status}: {body}")
            }
            Self::InvalidResponse { message, body } => {
                write!(
                    f,
                    "N2YO response did not match the expected schema: {message}; body: {body}"
                )
            }
        }
    }
}

impl Error for N2yoClientError {}

#[derive(Deserialize)]
struct PositionsResponse {
    positions: Vec<PositionPoint>,
}

#[derive(Deserialize)]
struct PositionPoint {
    satlatitude: f64,
    satlongitude: f64,
    sataltitude: f64,
    timestamp: i64,
}

#[derive(Deserialize)]
struct AboveResponse {
    above: Vec<AboveSatellite>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AboveSatellite {
    satid: u32,
    satname: String,
    satlat: f64,
    satlng: f64,
    satalt: f64,
}

#[derive(Clone)]
struct OpenMeteoClient {
    http: reqwest::blocking::Client,
    elevation_url: String,
}

impl OpenMeteoClient {
    fn new(elevation_url: impl Into<String>, timeout_seconds: u64) -> Result<Self, String> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(timeout_seconds))
            .build()
            .map_err(|error| format!("failed to build Open-Meteo HTTP client: {error}"))?;

        Ok(Self {
            http,
            elevation_url: elevation_url.into(),
        })
    }

    fn elevation(
        &self,
        latitude: f64,
        longitude: f64,
    ) -> Result<ElevationResponse, OpenMeteoClientError> {
        let upstream: OpenMeteoElevationResponse =
            self.get_json(&[("latitude", latitude), ("longitude", longitude)])?;

        let altitude_m = upstream
            .elevation
            .first()
            .copied()
            .flatten()
            .ok_or(OpenMeteoClientError::MissingElevation)?;

        Ok(ElevationResponse { altitude_m })
    }

    fn get_json<T>(&self, query: &[(&str, f64)]) -> Result<T, OpenMeteoClientError>
    where
        T: DeserializeOwned,
    {
        let response = self
            .http
            .get(&self.elevation_url)
            .query(query)
            .send()
            .map_err(|error| OpenMeteoClientError::RequestFailed(error.to_string()))?;

        let status = response.status();
        let body = response
            .text()
            .map_err(|error| OpenMeteoClientError::RequestFailed(error.to_string()))?;

        if !status.is_success() {
            return Err(OpenMeteoClientError::UpstreamStatus { status, body });
        }

        serde_json::from_str(&body).map_err(|error| OpenMeteoClientError::InvalidResponse {
            message: error.to_string(),
            body,
        })
    }
}

#[derive(Debug)]
pub enum OpenMeteoClientError {
    RequestFailed(String),
    UpstreamStatus { status: StatusCode, body: String },
    InvalidResponse { message: String, body: String },
    MissingElevation,
}

impl fmt::Display for OpenMeteoClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RequestFailed(message) => write!(f, "Open-Meteo request failed: {message}"),
            Self::UpstreamStatus { status, body } => {
                write!(f, "Open-Meteo returned HTTP {status}: {body}")
            }
            Self::InvalidResponse { message, body } => {
                write!(
                    f,
                    "Open-Meteo response did not match the expected schema: {message}; body: {body}"
                )
            }
            Self::MissingElevation => write!(f, "Open-Meteo response did not include elevation"),
        }
    }
}

impl Error for OpenMeteoClientError {}

struct ElevationResponse {
    altitude_m: f64,
}

#[derive(Deserialize)]
struct OpenMeteoElevationResponse {
    elevation: Vec<Option<f64>>,
}

fn validate_observer(lat: f64, lng: f64) -> Result<(), SatelliteDataError> {
    if !(-90.0..=90.0).contains(&lat) {
        return Err(SatelliteDataError::Validation(
            "observer latitude must be between -90 and 90".to_owned(),
        ));
    }

    if !(-180.0..=180.0).contains(&lng) {
        return Err(SatelliteDataError::Validation(
            "observer longitude must be between -180 and 180".to_owned(),
        ));
    }

    Ok(())
}
