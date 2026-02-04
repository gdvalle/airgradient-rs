use core::sync::atomic::AtomicU32;
use embassy_net::Stack;
use embassy_time::Duration;
use esp_alloc as _;
use picoserve::response::IntoResponse;
use picoserve::{AppBuilder, AppRouter, Router, routing};

use crate::metrics::metrics_handler;
use crate::sensors::SharedSensorData;

const ROOT_RESPONSE: &str = "OK";

pub async fn root_handler() -> impl IntoResponse {
    ROOT_RESPONSE
}

pub const WEB_TASK_POOL_SIZE: usize = 2;

#[embassy_executor::task(pool_size = WEB_TASK_POOL_SIZE)]
pub async fn web_task(
    task_id: usize,
    stack: Stack<'static>,
    router: &'static AppRouter<Application>,
    config: &'static picoserve::Config,
) -> ! {
    let port = 80;
    let mut tcp_rx_buffer = [0; 1024];
    let mut tcp_tx_buffer = [0; 1024];
    let mut http_buffer = [0; 2048];

    picoserve::Server::new(router, config, &mut http_buffer)
        .listen_and_serve(task_id, stack, port, &mut tcp_rx_buffer, &mut tcp_tx_buffer)
        .await
        .into_never()
}

pub struct WebApp {
    pub router: &'static Router<<Application as AppBuilder>::PathRouter>,
    pub config: &'static picoserve::Config,
}

impl WebApp {
    pub fn new(sensor_data: SharedSensorData, last_scrape_secs: &'static AtomicU32) -> Self {
        let app = Application {
            sensor_data,
            device_info: crate::device::DeviceInfo::get(),
            reset_reason: crate::device::resolve_reset_reason(esp_hal::system::reset_reason()),
            last_scrape_secs,
        };
        let router = picoserve::make_static!(AppRouter<Application>, app.build_app());

        let config = picoserve::make_static!(
            picoserve::Config,
            picoserve::Config::new(picoserve::Timeouts {
                start_read_request: Duration::from_secs(10),
                read_request: Duration::from_secs(10),
                write: Duration::from_secs(5),
                persistent_start_read_request: Duration::from_secs(5),
            })
            .keep_connection_alive()
        );

        Self { router, config }
    }
}

#[derive(Clone)]
pub struct Application {
    pub sensor_data: SharedSensorData,
    pub device_info: crate::device::DeviceInfo,
    pub reset_reason: &'static str,
    pub last_scrape_secs: &'static AtomicU32,
}

impl AppBuilder for Application {
    type PathRouter = impl picoserve::routing::PathRouter;

    fn build_app(self) -> Router<Self::PathRouter> {
        let Self {
            sensor_data,
            device_info,
            reset_reason,
            last_scrape_secs,
        } = self;
        picoserve::Router::new()
            .route("/", routing::get(root_handler))
            .route(
                "/metrics",
                routing::get(move || {
                    metrics_handler(
                        sensor_data,
                        device_info.clone(),
                        reset_reason,
                        last_scrape_secs,
                    )
                }),
            )
    }
}
