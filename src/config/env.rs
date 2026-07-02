use std::{
    error::Error,
    fmt::{self, Display, Formatter},
};

use config::{Config, Environment};
use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub n2yo_api_key: String,
    pub n2yo_base_url: String,
    pub n2yo_timeout_seconds: u64,
    pub open_meteo_elevation_url: String,
    pub open_meteo_timeout_seconds: u64,
    pub default_search_radius: u8,
    pub default_category_id: u32,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, AppConfigError> {
        dotenvy::dotenv().ok();

        let settings = Config::builder()
            .set_default("n2yo_api_key", "")
            .map_err(|source| AppConfigError::InvalidEnvironment(source.to_string()))?
            .set_default("n2yo_base_url", "https://api.n2yo.com/rest/v1/satellite")
            .map_err(|source| AppConfigError::InvalidEnvironment(source.to_string()))?
            .set_default("n2yo_timeout_seconds", 10)
            .map_err(|source| AppConfigError::InvalidEnvironment(source.to_string()))?
            .set_default(
                "open_meteo_elevation_url",
                "https://api.open-meteo.com/v1/elevation",
            )
            .map_err(|source| AppConfigError::InvalidEnvironment(source.to_string()))?
            .set_default("open_meteo_timeout_seconds", 10)
            .map_err(|source| AppConfigError::InvalidEnvironment(source.to_string()))?
            .set_default("default_search_radius", 70)
            .map_err(|source| AppConfigError::InvalidEnvironment(source.to_string()))?
            .set_default("default_category_id", 0)
            .map_err(|source| AppConfigError::InvalidEnvironment(source.to_string()))?
            .add_source(Environment::with_prefix("REONA"))
            .add_source(Environment::default())
            .build()
            .map_err(|source| AppConfigError::InvalidEnvironment(source.to_string()))?;

        let raw: RawAppConfig = settings
            .try_deserialize()
            .map_err(|source| AppConfigError::InvalidEnvironment(source.to_string()))?;

        raw.try_into()
    }
}

#[derive(Debug, Deserialize)]
struct RawAppConfig {
    n2yo_api_key: String,
    n2yo_base_url: String,
    n2yo_timeout_seconds: u64,
    open_meteo_elevation_url: String,
    open_meteo_timeout_seconds: u64,
    default_search_radius: u8,
    default_category_id: u32,
}

impl TryFrom<RawAppConfig> for AppConfig {
    type Error = AppConfigError;

    fn try_from(raw: RawAppConfig) -> Result<Self, Self::Error> {
        if raw.n2yo_api_key.trim().is_empty() {
            return Err(AppConfigError::InvalidField {
                field: "N2YO_API_KEY",
                message: "must not be empty".to_owned(),
            });
        }

        if raw.n2yo_base_url.trim().is_empty() {
            return Err(AppConfigError::InvalidField {
                field: "N2YO_BASE_URL",
                message: "must not be empty".to_owned(),
            });
        }

        if !raw.n2yo_base_url.starts_with("http://") && !raw.n2yo_base_url.starts_with("https://") {
            return Err(AppConfigError::InvalidField {
                field: "N2YO_BASE_URL",
                message: "must start with http:// or https://".to_owned(),
            });
        }

        if raw.n2yo_timeout_seconds == 0 {
            return Err(AppConfigError::InvalidField {
                field: "N2YO_TIMEOUT_SECONDS",
                message: "must be greater than 0".to_owned(),
            });
        }

        if raw.open_meteo_elevation_url.trim().is_empty() {
            return Err(AppConfigError::InvalidField {
                field: "OPEN_METEO_ELEVATION_URL",
                message: "must not be empty".to_owned(),
            });
        }

        if !raw.open_meteo_elevation_url.starts_with("http://")
            && !raw.open_meteo_elevation_url.starts_with("https://")
        {
            return Err(AppConfigError::InvalidField {
                field: "OPEN_METEO_ELEVATION_URL",
                message: "must start with http:// or https://".to_owned(),
            });
        }

        if raw.open_meteo_timeout_seconds == 0 {
            return Err(AppConfigError::InvalidField {
                field: "OPEN_METEO_TIMEOUT_SECONDS",
                message: "must be greater than 0".to_owned(),
            });
        }

        if raw.default_search_radius > 90 {
            return Err(AppConfigError::InvalidField {
                field: "REONA_DEFAULT_SEARCH_RADIUS",
                message: "must be between 0 and 90".to_owned(),
            });
        }

        Ok(AppConfig {
            n2yo_api_key: raw.n2yo_api_key,
            n2yo_base_url: raw.n2yo_base_url.trim_end_matches('/').to_owned(),
            n2yo_timeout_seconds: raw.n2yo_timeout_seconds,
            open_meteo_elevation_url: raw.open_meteo_elevation_url,
            open_meteo_timeout_seconds: raw.open_meteo_timeout_seconds,
            default_search_radius: raw.default_search_radius,
            default_category_id: raw.default_category_id,
        })
    }
}

#[derive(Debug)]
pub enum AppConfigError {
    InvalidEnvironment(String),
    InvalidField {
        field: &'static str,
        message: String,
    },
}

impl Display for AppConfigError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidEnvironment(message) => {
                write!(f, "failed to deserialize environment: {message}")
            }
            Self::InvalidField { field, message } => {
                write!(f, "invalid {field}: {message}")
            }
        }
    }
}

impl Error for AppConfigError {}
