use core::fmt::Debug;

const FRAME_START_1: u8 = 0x42;
const FRAME_START_2: u8 = 0x4D;
const EXPECTED_FRAME_LEN: u16 = 28;
const DATA_PAYLOAD_LEN: usize = 26; // Excludes checksum
const MAX_SYNC_ATTEMPTS: u32 = 2048;

#[derive(Debug, Copy, Clone)]
pub enum PmsError {
    Read,
    Checksum,
    FrameLen,
    MaxAttemptsExceeded, // More specific than generic Read error
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PmsData {
    pub(crate) pm1_ae: u16,
    pub(crate) pm25_ae: u16,
    pub(crate) pm10_ae: u16,
    pub(crate) pm03_count: u16,
    pub(crate) pm05_count: u16,
    pub(crate) pm10_count: u16,
    pub(crate) pm25_count: u16,
    pub(crate) temp: f32,
    pub(crate) humidity: f32,
}

pub struct Pms5003t<UART> {
    uart: UART,
}

impl<UART: embedded_io_async::Read> Pms5003t<UART> {
    pub fn new(uart: UART) -> Self {
        Self { uart }
    }

    pub(crate) async fn read(&mut self) -> Result<PmsData, PmsError> {
        for _ in 0..MAX_SYNC_ATTEMPTS {
            if let Ok(data) = self.try_read_frame().await {
                return Ok(data);
            }
        }
        Err(PmsError::MaxAttemptsExceeded)
    }

    async fn try_read_frame(&mut self) -> Result<PmsData, PmsError> {
        // Sync to frame start
        if self.read_byte().await? != FRAME_START_1 {
            return Err(PmsError::Read);
        }
        if self.read_byte().await? != FRAME_START_2 {
            return Err(PmsError::Read);
        }

        // Read frame length
        let mut len_buf = [0u8; 2];
        self.read_exact(&mut len_buf).await?;

        let frame_len = u16::from_be_bytes(len_buf);
        if frame_len != EXPECTED_FRAME_LEN {
            return Err(PmsError::FrameLen);
        }

        // Read data payload
        let mut data_buf = [0u8; 28];
        self.read_exact(&mut data_buf).await?;

        // Verify checksum
        if !Self::verify_checksum(&len_buf, &data_buf) {
            return Err(PmsError::Checksum);
        }

        Ok(Self::parse_frame(&data_buf))
    }

    fn verify_checksum(len_buf: &[u8; 2], data_buf: &[u8; 28]) -> bool {
        let mut sum: u16 = FRAME_START_1 as u16 + FRAME_START_2 as u16;
        sum = sum.wrapping_add(len_buf[0] as u16);
        sum = sum.wrapping_add(len_buf[1] as u16);

        for &byte in data_buf.iter().take(DATA_PAYLOAD_LEN) {
            sum = sum.wrapping_add(byte as u16);
        }

        let expected_sum = u16::from_be_bytes([data_buf[26], data_buf[27]]);
        sum == expected_sum
    }

    fn parse_frame(data_buf: &[u8; 28]) -> PmsData {
        let pm1_ae = u16::from_be_bytes([data_buf[6], data_buf[7]]);
        let pm25_ae = u16::from_be_bytes([data_buf[8], data_buf[9]]);
        let pm10_ae = u16::from_be_bytes([data_buf[10], data_buf[11]]);
        let pm03_count = u16::from_be_bytes([data_buf[12], data_buf[13]]);
        let pm05_count = u16::from_be_bytes([data_buf[14], data_buf[15]]);
        let pm10_count = u16::from_be_bytes([data_buf[16], data_buf[17]]);
        let pm25_count = u16::from_be_bytes([data_buf[18], data_buf[19]]);

        let temp_raw = i16::from_be_bytes([data_buf[20], data_buf[21]]);
        let hum_raw = u16::from_be_bytes([data_buf[22], data_buf[23]]);

        PmsData {
            pm1_ae,
            pm25_ae,
            pm10_ae,
            pm03_count,
            pm05_count,
            pm10_count,
            pm25_count,
            temp: (temp_raw as f32) / 10.0,
            humidity: (hum_raw as f32) / 10.0,
        }
    }

    async fn read_byte(&mut self) -> Result<u8, PmsError> {
        let mut buf = [0u8; 1];
        self.uart
            .read_exact(&mut buf)
            .await
            .map_err(|_| PmsError::Read)?;
        Ok(buf[0])
    }

    async fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), PmsError> {
        self.uart.read_exact(buf).await.map_err(|_| PmsError::Read)
    }
}
