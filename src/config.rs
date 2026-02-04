//! Configuration constants for the AirGradient firmware.
//!
//! This module centralizes all configuration values that are set via
//! environment variables at compile time or hardcoded constants.

use embassy_time::Duration;

/// WiFi configuration settings.
#[derive(Debug, Clone, Copy)]
pub struct WifiConfig {
    /// WiFi SSID to connect to.
    pub ssid: Option<&'static str>,
    /// WiFi password for authentication.
    pub password: Option<&'static str>,
    /// Whether to perform a WiFi scan on startup.
    pub scan: bool,
    /// Power saving mode for WiFi.
    pub power_save_mode: esp_radio::wifi::PowerSaveMode,
}

/// Watchdog configuration settings.
#[derive(Debug, Clone, Copy)]
pub struct WatchdogConfig {
    /// How often the watchdog task checks system health.
    pub tick_interval: Duration,
    /// Maximum time WiFi can be down before the system is considered unhealthy.
    pub wifi_timeout: Duration,
    /// Maximum age of sensor data before the system is considered unhealthy.
    pub sensor_timeout: Duration,
    /// Maximum time since last metric scrape before the system is considered unhealthy.
    pub metric_scrape_timeout: Duration,
    /// Duration of the watchdog kick pulse (HIGH state) in milliseconds.
    pub kick_duration_ms: u32,
}

/// Sensor configuration settings.
#[derive(Debug, Clone, Copy)]
pub struct SensorConfig {
    /// The interval at which sensors are polled.
    pub polling_interval: Duration,
}

/// Global application configuration.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// WiFi configuration.
    pub wifi: WifiConfig,
    /// Watchdog configuration.
    pub watchdog: WatchdogConfig,
    /// Sensor configuration.
    pub sensor: SensorConfig,
    /// Whether to print heap and network status in the main loop.
    pub print_status_loop: bool,
}

impl Config {
    /// Creates a new configuration from compile-time environment variables.
    const fn new() -> Self {
        Self {
            wifi: WifiConfig {
                ssid: option_env!("WIFI_SSID"),
                password: option_env!("WIFI_PASSWORD"),
                scan: matches!(option_env!("WIFI_SCAN"), Some("true")),
                power_save_mode: match option_env!("WIFI_POWER_SAVE_MODE") {
                    Some("0") => esp_radio::wifi::PowerSaveMode::None,
                    Some("1") => esp_radio::wifi::PowerSaveMode::Minimum,
                    Some("2") => esp_radio::wifi::PowerSaveMode::Maximum,
                    Some(_) => panic!("Invalid WIFI_POWER_SAVE_MODE value"),
                    None => esp_radio::wifi::PowerSaveMode::Minimum,
                },
            },
            watchdog: WatchdogConfig {
                tick_interval: Duration::from_secs(60),
                wifi_timeout: Duration::from_secs(300), // 5 minutes
                sensor_timeout: Duration::from_secs(120), // 2 minutes
                metric_scrape_timeout: Duration::from_secs(900), // 15 minutes
                kick_duration_ms: 25,
            },
            sensor: SensorConfig {
                polling_interval: Duration::from_secs(2),
            },
            print_status_loop: matches!(option_env!("PRINT_STATUS_LOOP"), Some("true")),
        }
    }
}

/// Global configuration instance.
pub static CONFIG: Config = Config::new();
