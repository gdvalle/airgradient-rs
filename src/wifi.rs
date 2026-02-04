use embassy_executor::Spawner;
use embassy_net::{DhcpConfig, Runner, Stack, StackResources};
use embassy_time::{Duration, Timer};
use esp_hal::rng::Rng;
use esp_radio::wifi::{
    ClientConfig, ModeConfig, ScanConfig, WifiController, WifiDevice, WifiEvent, WifiStaState,
};

use crate::config::CONFIG;

const STACK_RESOURCES_SIZE: usize = 8;

#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>) {
    let ssid = CONFIG.wifi.ssid.unwrap_or("");
    let password = CONFIG.wifi.password.unwrap_or("");

    if ssid.is_empty() {
        defmt::info!("wifi: WIFI_SSID env var not specified!");
    };

    defmt::info!(
        "wifi: Device capabilities: {:?}",
        defmt::Debug2Format(&controller.capabilities())
    );
    loop {
        if esp_radio::wifi::sta_state() == WifiStaState::Connected {
            // wait until we're no longer connected
            defmt::info!("wifi: Waiting for disconnection...");
            controller.wait_for_event(WifiEvent::StaDisconnected).await;
            defmt::info!("wifi: Disconnected");
            Timer::after(Duration::from_millis(5000)).await;
        }
        if !matches!(controller.is_started(), Ok(true)) {
            let mut client_config = ClientConfig::default().with_ssid(ssid.into());
            if !password.is_empty() {
                client_config = client_config.with_password(password.into());
            }

            let mode_config = ModeConfig::Client(client_config);
            controller.set_config(&mode_config).unwrap();
            defmt::info!("wifi: Starting...");
            controller.start_async().await.unwrap();
            defmt::info!("wifi: Started!");

            if CONFIG.wifi.scan {
                defmt::info!("wifi: Scanning...");
                let scan_config = ScanConfig::default().with_max(10);
                let result = controller
                    .scan_with_config_async(scan_config)
                    .await
                    .unwrap();
                for ap in result {
                    defmt::info!("wifi: Found AP: {:?}", defmt::Debug2Format(&ap));
                }
            }
        }
        defmt::info!("wifi: Connecting...");

        match controller.connect_async().await {
            Ok(_) => defmt::info!("wifi: Connected!"),
            Err(e) => {
                defmt::info!("wifi: Failed to connect: {:?}", defmt::Debug2Format(&e));
                Timer::after(Duration::from_millis(5000)).await
            }
        }
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, WifiDevice<'static>>) {
    runner.run().await
}

pub async fn start_wifi(
    radio_init: &'static esp_radio::Controller<'static>,
    wifi: esp_hal::peripherals::WIFI<'static>,
    rng: Rng,
    spawner: &Spawner,
) -> Stack<'static> {
    let wifi_config =
        esp_radio::wifi::Config::default().with_power_save_mode(CONFIG.wifi.power_save_mode);

    let (wifi_controller, interfaces) = esp_radio::wifi::new(radio_init, wifi, wifi_config)
        .expect("Failed to initialize wifi controller");

    let wifi_interface = interfaces.sta;
    let net_seed = rng.random() as u64 | ((rng.random() as u64) << 32);

    let dhcp_config = DhcpConfig::default();
    let net_config = embassy_net::Config::dhcpv4(dhcp_config);

    // Init network stack
    let (stack, runner) = embassy_net::new(
        wifi_interface,
        net_config,
        picoserve::make_static!(
            StackResources<STACK_RESOURCES_SIZE>,
            StackResources::<STACK_RESOURCES_SIZE>::new()
        ),
        net_seed,
    );

    spawner.spawn(connection(wifi_controller)).ok();
    spawner.spawn(net_task(runner)).ok();

    defmt::info!("wifi: Waiting for link to be up");
    stack.wait_link_up().await;
    stack.wait_config_up().await;

    defmt::info!(
        "wifi: Got IP: {}",
        defmt::Display2Format(&stack.config_v4().unwrap().address)
    );

    // unsafe {
    //    defmt::info!("wifi: Setting max TX power to 8");
    //     esp_wifi_sys::include::esp_wifi_set_max_tx_power(8);
    // }

    stack
}
