#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(impl_trait_in_assoc_type)]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use core::sync::atomic::AtomicU32;

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_backtrace as _; // Register the panic handler.
use esp_hal::clock::CpuClock;
use esp_hal::rng::Rng;
use esp_hal::timer::timg::TimerGroup;
use esp_println as _; // Register the defmt UART global logger.
// use defmt_rtt as _; // Register the defmt RTT global logger.

use airgradient as lib;

// This creates a default app-descriptor required by the esp-idf bootloader.
esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    defmt::info!("Init...");

    let reset_reason = esp_hal::system::reset_reason();
    defmt::info!("Reset Reason: {:?}", defmt::Debug2Format(&reset_reason));

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Reclaim internal memory reserved from startup.
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 64 * 1024);
    // Allocate regular SRAM for a reasonable amount of total heap.
    // Per https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-guides/memory-types.html
    // we should be able to allocate up to 160KiB?
    // At the time of writing, we use ~47K of heap in steady state.
    esp_alloc::heap_allocator!(size: 48 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);

    let sw_int =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);

    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    defmt::info!("RTOS scheduler initialized");

    let radio_init = picoserve::make_static!(
        esp_radio::Controller<'static>,
        esp_radio::init().expect("Failed to initialize Wi-Fi/BLE controller")
    );

    let delay = esp_hal::delay::Delay::new();

    // External Watchdog (TPL5010) Kick on GPIO2.
    let watchdog_pin = esp_hal::gpio::Output::new(
        peripherals.GPIO2,
        esp_hal::gpio::Level::Low,
        esp_hal::gpio::OutputConfig::default().with_pull(esp_hal::gpio::Pull::Down),
    );

    let rng = Rng::new();

    let stack = lib::wifi::start_wifi(radio_init, peripherals.WIFI, rng, &spawner).await;

    // Sensor Initialization
    // SAFETY: We are in main and these peripherals are singletons.
    // We "steal" them using ptr::read to bypass the local lifetime
    // from esp_hal::init and satisfy the 'static demand of the task.
    let i2c0 = unsafe {
        esp_hal::i2c::master::I2c::new(
            core::ptr::read(&peripherals.I2C0),
            esp_hal::i2c::master::Config::default(),
        )
        .unwrap()
        .with_sda(core::ptr::read(&peripherals.GPIO7))
        .with_scl(core::ptr::read(&peripherals.GPIO6))
        .into_async()
    };

    let sgp = lib::sensors::sgp41::Sgp41::new(
        i2c0,
        (lib::config::CONFIG.sensor.polling_interval.as_millis() as f32) / 1000.0,
    );
    let uart0_config = esp_hal::uart::Config::default().with_baudrate(9600);
    let uart0 = unsafe {
        esp_hal::uart::Uart::new(core::ptr::read(&peripherals.UART0), uart0_config)
            .unwrap()
            .with_rx(core::ptr::read(&peripherals.GPIO20))
            .with_tx(core::ptr::read(&peripherals.GPIO21))
            .into_async()
    };
    let pms = lib::sensors::pms5003t::Pms5003t::new(uart0);

    let uart1_config = esp_hal::uart::Config::default().with_baudrate(9600);
    let uart1 = unsafe {
        esp_hal::uart::Uart::new(core::ptr::read(&peripherals.UART1), uart1_config)
            .unwrap()
            .with_rx(core::ptr::read(&peripherals.GPIO0))
            .with_tx(core::ptr::read(&peripherals.GPIO1))
            .into_async()
    };
    let s8 = lib::sensors::s8::S8::new(uart1);

    let sensor_manager = lib::sensors::SensorManager::new(sgp, pms, s8);
    let sensor_data = lib::sensors::SharedSensorData::new();
    spawner.must_spawn(lib::sensors::sensor_task(sensor_manager, sensor_data));

    let last_scrape_secs = picoserve::make_static!(AtomicU32, AtomicU32::new(0));

    spawner.must_spawn(lib::watchdog::watchdog_task(
        watchdog_pin,
        delay,
        stack,
        sensor_data,
        last_scrape_secs,
    ));

    let web_app = lib::web::WebApp::new(sensor_data, last_scrape_secs);
    for id in 0..lib::web::WEB_TASK_POOL_SIZE {
        spawner.must_spawn(lib::web::web_task(
            id,
            stack,
            web_app.router,
            web_app.config,
        ));
    }

    loop {
        if lib::config::CONFIG.print_status_loop {
            let uptime = embassy_time::Instant::now().as_secs();
            let stats = esp_alloc::HEAP.stats();

            defmt::info!(
                "[{}s] Link: {} | IP: {} | Heap: {}/{} ({} free)",
                uptime,
                stack.is_link_up(),
                stack.config_v4().map(|c| c.address).is_some(),
                stats.current_usage,
                stats.size,
                stats.size - stats.current_usage
            );
        }

        Timer::after(Duration::from_secs(1)).await;
    }
}
