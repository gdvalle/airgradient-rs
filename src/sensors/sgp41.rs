const CRC_POLYNOMIAL: u8 = 0x31;
use gas_index_algorithm::{AlgorithmType, GasIndexAlgorithm};
const CRC_INIT: u8 = 0xFF;
const SGP41_ADDRESS: u8 = 0x59;

// Commands
const CMD_MEASURE_RAW: [u8; 2] = [0x26, 0x19];
const CMD_SELF_TEST: [u8; 2] = [0x28, 0x0E];
const CMD_HEATER_OFF: [u8; 2] = [0x36, 0x15];

// Default compensation values
const DEFAULT_RH_TICKS: u16 = 0x8000; // 50% RH
const DEFAULT_TEMP_TICKS: u16 = 0x6666; // 25°C

// Measurement timing
const MEASURE_DELAY_MS: u32 = 50;
const SELF_TEST_DELAY_MS: u32 = 320; // Self-test takes ~320ms
const CONDITIONING_DELAY_MS: u32 = 10_000; // 10 seconds recommended

// Buffer sizes
const RESPONSE_SIZE: usize = 6; // 2 bytes data + 1 CRC, twice
const SELF_TEST_RESPONSE_SIZE: usize = 3; // 2 bytes result + 1 CRC

// Self-test result codes
const SELF_TEST_OK: u16 = 0xD400;

pub struct Sgp41<I2C> {
    i2c: I2C,
    address: u8,
    initialized: bool,
    voc_algorithm: GasIndexAlgorithm,
    nox_algorithm: GasIndexAlgorithm,
}

#[derive(Debug, Copy, Clone)]
pub enum Sgp41Error {
    I2cError,
    CrcError,
    SelfTestFailed(u16), // Contains the actual test result
    NotInitialized,
}

impl<I2C> Sgp41<I2C>
where
    I2C: embedded_hal_async::i2c::I2c,
{
    /// Create a new SGP41 driver instance
    ///
    /// Note: You must call `init()` before taking measurements
    pub fn new(i2c: I2C, sampling_interval_secs: f32) -> Self {
        Self {
            i2c,
            address: SGP41_ADDRESS,
            initialized: false,
            voc_algorithm: GasIndexAlgorithm::new(AlgorithmType::Voc, sampling_interval_secs),
            nox_algorithm: GasIndexAlgorithm::new(AlgorithmType::Nox, sampling_interval_secs),
        }
    }

    /// Initialize the sensor with self-test and conditioning
    ///
    /// This performs:
    /// 1. Self-test to verify sensor functionality
    /// 2. Conditioning period (10 seconds) to stabilize readings
    ///
    /// Must be called once after power-on before measurements
    pub async fn init(&mut self) -> Result<(), Sgp41Error> {
        // Run self-test
        self.self_test().await?;

        // Conditioning: Run measurements for 10 seconds to stabilize
        // Take one measurement, then wait the remainder of the conditioning time
        let _ = self.measure_internal(None, None).await?;

        // Wait for conditioning period (minus the measurement time)
        embassy_time::Timer::after_millis((CONDITIONING_DELAY_MS - MEASURE_DELAY_MS) as u64).await;

        self.initialized = true;
        Ok(())
    }

    /// Run the sensor's built-in self-test
    ///
    /// Tests the sensor's heater and measurement circuitry.
    /// Returns Ok if test passes, Err with result code if it fails.
    pub async fn self_test(&mut self) -> Result<(), Sgp41Error> {
        // Send self-test command
        self.i2c
            .write(self.address, &CMD_SELF_TEST)
            .await
            .map_err(|_| Sgp41Error::I2cError)?;

        // Wait for self-test to complete (~320ms)
        embassy_time::Timer::after_millis(SELF_TEST_DELAY_MS as u64).await;

        // Read result
        let mut read_buf = [0u8; SELF_TEST_RESPONSE_SIZE];
        self.i2c
            .read(self.address, &mut read_buf)
            .await
            .map_err(|_| Sgp41Error::I2cError)?;

        // Validate CRC
        Self::validate_crc(&read_buf[0..2], read_buf[2])?;

        // Check test result
        let test_result = u16::from_be_bytes([read_buf[0], read_buf[1]]);
        if test_result == SELF_TEST_OK {
            Ok(())
        } else {
            Err(Sgp41Error::SelfTestFailed(test_result))
        }
    }

    /// Turn off the heater (for power saving when not in use)
    ///
    /// After calling this, you must call `init()` again before measurements
    pub async fn heater_off(&mut self) -> Result<(), Sgp41Error> {
        self.i2c
            .write(self.address, &CMD_HEATER_OFF)
            .await
            .map_err(|_| Sgp41Error::I2cError)?;

        self.initialized = false;
        Ok(())
    }

    /// Check if the sensor has been initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Measure raw VOC and NOx signals with optional temperature and humidity compensation
    ///
    /// # Arguments
    /// * `humidity` - Relative humidity in % (0-100). Defaults to 50% if None.
    /// * `temp` - Temperature in °C (-45 to 130). Defaults to 25°C if None.
    ///
    /// # Returns
    /// * `Ok((voc_raw, nox_raw))` - Raw sensor values
    /// * `Err(Sgp41Error)` - Communication, CRC, or initialization error
    ///
    /// # Important
    /// You must call `init()` once before using this method
    pub async fn measure(
        &mut self,
        humidity: Option<f32>,
        temp: Option<f32>,
    ) -> Result<(u16, u16), Sgp41Error> {
        if !self.initialized {
            return Err(Sgp41Error::NotInitialized);
        }

        self.measure_internal(humidity, temp).await
    }

    /// Measure VOC and NOx indices with optional temperature and humidity compensation
    ///
    /// This method wraps `measure` and processes raw values through the Gas Index Algorithm.
    ///
    /// # Arguments
    /// * `humidity` - Relative humidity in % (0-100). Defaults to 50% if None.
    /// * `temp` - Temperature in °C (-45 to 130). Defaults to 25°C if None.
    ///
    /// # Returns
    /// * `Ok((voc_index, nox_index))` - Processed index values
    /// * `Err(Sgp41Error)` - Communication, CRC, or initialization error
    pub async fn measure_indices(
        &mut self,
        humidity: Option<f32>,
        temp: Option<f32>,
    ) -> Result<(i32, i32), Sgp41Error> {
        let (voc_raw, nox_raw) = self.measure(humidity, temp).await?;

        let voc_index = self.voc_algorithm.process(voc_raw as i32);
        let nox_index = self.nox_algorithm.process(nox_raw as i32);

        Ok((voc_index, nox_index))
    }

    /// Internal measurement function (bypasses initialization check)
    pub async fn measure_internal(
        &mut self,
        humidity: Option<f32>,
        temp: Option<f32>,
    ) -> Result<(u16, u16), Sgp41Error> {
        let rh_ticks = humidity
            .map(|h| Self::humidity_to_ticks(h))
            .unwrap_or(DEFAULT_RH_TICKS);

        let t_ticks = temp
            .map(|t| Self::temperature_to_ticks(t))
            .unwrap_or(DEFAULT_TEMP_TICKS);

        let rh_bytes = rh_ticks.to_be_bytes();
        let rh_crc = crc8(&rh_bytes);
        let t_bytes = t_ticks.to_be_bytes();
        let t_crc = crc8(&t_bytes);

        let buffer = [
            CMD_MEASURE_RAW[0],
            CMD_MEASURE_RAW[1],
            rh_bytes[0],
            rh_bytes[1],
            rh_crc,
            t_bytes[0],
            t_bytes[1],
            t_crc,
        ];

        self.i2c
            .write(self.address, &buffer)
            .await
            .map_err(|_| Sgp41Error::I2cError)?;

        embassy_time::Timer::after_millis(MEASURE_DELAY_MS as u64).await;

        let mut read_buf = [0u8; RESPONSE_SIZE];
        self.i2c
            .read(self.address, &mut read_buf)
            .await
            .map_err(|_| Sgp41Error::I2cError)?;

        // Validate CRCs
        Self::validate_crc(&read_buf[0..2], read_buf[2])?;
        Self::validate_crc(&read_buf[3..5], read_buf[5])?;

        let voc_raw = u16::from_be_bytes([read_buf[0], read_buf[1]]);
        let nox_raw = u16::from_be_bytes([read_buf[3], read_buf[4]]);

        Ok((voc_raw, nox_raw))
    }

    /// Convert relative humidity percentage to SGP41 ticks
    /// Formula: RH ticks = %RH * 65535 / 100
    fn humidity_to_ticks(humidity: f32) -> u16 {
        let h = humidity.clamp(0.0, 100.0);
        (h * 65535.0 / 100.0) as u16
    }

    /// Convert temperature in Celsius to SGP41 ticks
    /// Formula: T ticks = (°C + 45) * 65535 / 175
    fn temperature_to_ticks(temp: f32) -> u16 {
        let t = temp.clamp(-45.0, 130.0);
        ((t + 45.0) * 65535.0 / 175.0) as u16
    }

    /// Validate CRC8 checksum for received data
    fn validate_crc(data: &[u8], expected_crc: u8) -> Result<(), Sgp41Error> {
        if crc8(data) == expected_crc {
            Ok(())
        } else {
            Err(Sgp41Error::CrcError)
        }
    }
}

fn crc8(data: &[u8]) -> u8 {
    let mut crc = CRC_INIT;
    for &byte in data {
        crc ^= byte;
        for _ in 0..8 {
            if (crc & 0x80) != 0 {
                crc = (crc << 1) ^ CRC_POLYNOMIAL;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}
