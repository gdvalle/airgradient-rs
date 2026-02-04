use crate::config::CONFIG;
use crate::sensors::SharedSensorData;

use core::sync::atomic::{AtomicU32, Ordering};
use embassy_net::Stack;
use embassy_time::{Instant, Ticker};
use esp_hal::delay::Delay;

#[embassy_executor::task]
pub async fn watchdog_task(
    mut watchdog_pin: esp_hal::gpio::Output<'static>,
    delay: Delay,
    stack: Stack<'static>,
    sensors: SharedSensorData,
    last_scrape_secs: &'static AtomicU32,
) {
    // TPL5010 usually expects a pulse. The Arduino code does HIGH -> 25ms -> LOW.
    // Ensure we start in a known state (usually LOW for TPL5010 "DONE" pin)
    watchdog_pin.set_low();
    let mut ticker = Ticker::every(CONFIG.watchdog.tick_interval);
    let mut last_wifi_ok = Instant::now();

    loop {
        ticker.next().await;
        let now = Instant::now();
        let mut healthy = true;

        // Wifi
        if stack.is_link_up() && stack.is_config_up() {
            last_wifi_ok = now;
        } else if now.duration_since(last_wifi_ok) > CONFIG.watchdog.wifi_timeout {
            defmt::info!(
                "Watchdog: WiFi down for > {:?}",
                defmt::Display2Format(&CONFIG.watchdog.wifi_timeout)
            );
            healthy = false;
        }

        // Sensors
        let sensor_last_updated = sensors.lock().await.last_updated;
        if now.duration_since(sensor_last_updated) > CONFIG.watchdog.sensor_timeout {
            defmt::info!(
                "Watchdog: Sensors stale (Age: {:?})",
                defmt::Display2Format(&now.duration_since(sensor_last_updated))
            );
            healthy = false;
        }

        // Metrics scrape
        let last_scrape = last_scrape_secs.load(Ordering::Relaxed);
        let scrape_age = now.duration_since(Instant::from_secs(last_scrape as u64));
        if scrape_age > CONFIG.watchdog.metric_scrape_timeout {
            defmt::info!(
                "Watchdog: Metric scrape stale (Age: {:?})",
                defmt::Display2Format(&scrape_age)
            );
            healthy = false;
        }

        if healthy {
            // Perform the "kick"
            watchdog_pin.set_high();
            delay.delay_millis(CONFIG.watchdog.kick_duration_ms);
            watchdog_pin.set_low();

            defmt::info!("Fed external watchdog");
        } else {
            defmt::info!("System UNHEALTHY. Skipping watchdog kick.");
        }
    }
}
