use crate::sensors::{SensorManager, SharedSensorData};
use embassy_time::Timer;

#[embassy_executor::task]
pub async fn sensor_task(
    mut manager: SensorManager<
        esp_hal::i2c::master::I2c<'static, esp_hal::Async>,
        esp_hal::uart::Uart<'static, esp_hal::Async>,
        esp_hal::uart::Uart<'static, esp_hal::Async>,
    >,
    sensor_data: SharedSensorData,
) -> ! {
    // Initialize sensors (e.g. SGP41 self-test and conditioning)
    defmt::info!("Initializing sensors...");
    let _ = manager.init().await;
    defmt::info!("Sensors initialized");

    loop {
        manager.read_and_update(&sensor_data).await;
        Timer::after(crate::config::CONFIG.sensor.polling_interval).await;
    }
}
