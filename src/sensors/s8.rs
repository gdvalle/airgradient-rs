use core::fmt::Debug;

#[derive(Debug, Copy, Clone)]
pub enum S8Error {
    ReadError,
    WriteError,
    ChecksumError,
    InvalidHeader,
}

pub struct S8<UART> {
    uart: UART,
}

// Modbus protocol constants
const MODBUS_ADDR_ANY: u8 = 0xFE;
const MODBUS_FUNC_READ_INPUT: u8 = 0x04;
const MODBUS_IR4_CO2_HIGH: u8 = 0x00;
const MODBUS_IR4_CO2_LOW: u8 = 0x03;
const MODBUS_READ_LEN_HIGH: u8 = 0x00;
const MODBUS_READ_LEN_LOW: u8 = 0x01;
const RESPONSE_BYTE_COUNT: u8 = 0x02;

impl<UART: embedded_io_async::Read + embedded_io_async::Write> S8<UART> {
    pub fn new(uart: UART) -> Self {
        Self { uart }
    }

    pub(crate) async fn get_co2(&mut self) -> Result<u16, S8Error> {
        // Modbus command: Addr(0xFE), Func(0x04), Reg(0x0003), Len(0x0001), CRC
        // S8 uses 0xFE as "Any Address". IR4 (Input Register 4) is CO2.
        // IR4 is address 0x0003 (0-indexed).
        let mut cmd = [
            MODBUS_ADDR_ANY,
            MODBUS_FUNC_READ_INPUT,
            MODBUS_IR4_CO2_HIGH,
            MODBUS_IR4_CO2_LOW,
            MODBUS_READ_LEN_HIGH,
            MODBUS_READ_LEN_LOW,
            0x00, // CRC low byte (calculated below)
            0x00, // CRC high byte (calculated below)
        ];

        // Calculate and append CRC16 Modbus
        let crc = crc16_modbus(&cmd[0..6]);
        cmd[6] = (crc & 0xFF) as u8; // CRC low byte
        cmd[7] = ((crc >> 8) & 0xFF) as u8; // CRC high byte

        self.uart
            .write_all(&cmd)
            .await
            .map_err(|_| S8Error::WriteError)?;

        let mut buf = [0u8; 7];
        self.uart
            .read_exact(&mut buf)
            .await
            .map_err(|_| S8Error::ReadError)?;

        validate_response(&buf)?;

        let co2 = ((buf[3] as u16) << 8) | (buf[4] as u16);
        Ok(co2)
    }
}

fn validate_response(buf: &[u8; 7]) -> Result<(), S8Error> {
    if buf[0] != MODBUS_ADDR_ANY
        || buf[1] != MODBUS_FUNC_READ_INPUT
        || buf[2] != RESPONSE_BYTE_COUNT
    {
        return Err(S8Error::InvalidHeader);
    }

    // Modbus CRC is transmitted as [CRC_LOW, CRC_HIGH]
    // buf[5] = CRC_LOW, buf[6] = CRC_HIGH
    // Reconstruct as: (HIGH << 8) | LOW
    let received_crc = ((buf[6] as u16) << 8) | (buf[5] as u16);
    let calculated_crc = crc16_modbus(&buf[0..5]);

    if calculated_crc != received_crc {
        return Err(S8Error::ChecksumError);
    }

    Ok(())
}

fn crc16_modbus(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= byte as u16;
        for _ in 0..8 {
            if (crc & 0x0001) != 0 {
                crc = (crc >> 1) ^ 0xA001;
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}
