use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::Instant;

use static_cell::StaticCell;

use crate::sensors::pms5003t::PmsError;
use crate::sensors::s8::S8Error;
use crate::sensors::sgp41::Sgp41Error;

use crate::sensors;

#[derive(Debug, Clone)]
pub struct SensorData {
    pub pm1: u16,
    pub pm25: u16,
    pub pm10: u16,
    pub pm03_count: u16,
    pub pm05_count: u16,
    pub pm10_count: u16,
    pub pm25_count: u16,
    pub co2: u16,
    pub voc: i32,
    pub nox: i32,
    pub temp: f32,
    pub humidity: f32,
    pub initialized: bool,
    pub errors: Option<SensorErrors>,
    pub last_updated: Instant,
}

impl Default for SensorData {
    fn default() -> Self {
        Self {
            pm1: 0,
            pm25: 0,
            pm10: 0,
            pm03_count: 0,
            pm05_count: 0,
            pm10_count: 0,
            pm25_count: 0,
            co2: 0,
            voc: 0,
            nox: 0,
            temp: 0.0,
            humidity: 0.0,
            initialized: false,
            errors: None,
            last_updated: Instant::now(),
        }
    }
}

#[derive(Debug, Clone, Copy)] // Copy is cheap!
pub struct SensorErrors {
    pub pms: Option<PmsError>,
    pub sgp: Option<Sgp41Error>,
    pub s8: Option<S8Error>,
}

#[derive(Clone, Copy)]
pub struct SharedSensorData(&'static Mutex<CriticalSectionRawMutex, SensorData>);

impl SharedSensorData {
    pub fn new() -> Self {
        static SENSOR_DATA: StaticCell<Mutex<CriticalSectionRawMutex, SensorData>> =
            StaticCell::new();
        Self(SENSOR_DATA.init(Mutex::new(SensorData::default())))
    }

    pub async fn lock(
        &self,
    ) -> embassy_sync::mutex::MutexGuard<'_, CriticalSectionRawMutex, SensorData> {
        self.0.lock().await
    }

    pub async fn update(&self, data: SensorData) {
        let mut inner = self.0.lock().await;
        *inner = data;
    }
}

impl Default for SharedSensorData {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SensorManager<I2C, UART0, UART1> {
    sgp: sensors::sgp41::Sgp41<I2C>,
    pms: sensors::pms5003t::Pms5003t<UART0>,
    s8: sensors::s8::S8<UART1>,
}

impl<I2C, UART0, UART1> SensorManager<I2C, UART0, UART1>
where
    I2C: embedded_hal_async::i2c::I2c,
    UART0: embedded_io_async::Read,
    UART1: embedded_io_async::Read + embedded_io_async::Write,
{
    pub fn new(
        sgp: sensors::sgp41::Sgp41<I2C>,
        pms: sensors::pms5003t::Pms5003t<UART0>,
        s8: sensors::s8::S8<UART1>,
    ) -> Self {
        #[allow(clippy::as_conversions)]
        Self { sgp, pms, s8 }
    }

    pub async fn init(&mut self) -> Result<(), sensors::sgp41::Sgp41Error> {
        self.sgp.init().await
    }

    pub async fn read_and_update(&mut self, shared: &SharedSensorData) {
        let mut data = self.read_all().await;
        // Once initialized, keep it true. The SGP41 driver tracks its own state,
        // but here we ensure the shared data reflects it.
        data.initialized = self.sgp.is_initialized();
        shared.update(data).await;
    }

    async fn read_all(&mut self) -> SensorData {
        let mut data = SensorData::default();
        let mut error_flags = SensorErrors {
            pms: None,
            sgp: None,
            s8: None,
        };
        let mut has_error = false;

        // Read PMS first to get temp/humidity for compensation
        match self.pms.read().await {
            Ok(pms_data) => {
                data.pm1 = pms_data.pm1_ae;
                data.pm25 = pms_data.pm25_ae;
                data.pm10 = pms_data.pm10_ae;
                data.pm03_count = pms_data.pm03_count;
                data.pm05_count = pms_data.pm05_count;
                data.pm10_count = pms_data.pm10_count;
                data.pm25_count = pms_data.pm25_count;
                data.temp = pms_data.temp;
                data.humidity = pms_data.humidity;
            }
            Err(e) => {
                error_flags.pms = Some(e);
                has_error = true;
            }
        }

        // Use temp/humidity from PMS for SGP compensation if available
        match self
            .sgp
            .measure_indices(Some(data.humidity), Some(data.temp))
            .await
        {
            Ok((voc_idx, nox_idx)) => {
                data.voc = voc_idx;
                data.nox = nox_idx;
            }
            Err(e) => {
                error_flags.sgp = Some(e);
                has_error = true;
            }
        }

        match self.s8.get_co2().await {
            Ok(co2) => {
                data.co2 = co2;
            }
            Err(e) => {
                error_flags.s8 = Some(e);
                has_error = true;
            }
        }

        if has_error {
            data.errors = Some(error_flags);
        }

        data.initialized = self.sgp.is_initialized();
        data.last_updated = Instant::now();

        data
    }
}
