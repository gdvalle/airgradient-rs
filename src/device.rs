use core::fmt::Write;
use esp_hal::efuse::Efuse;

/// Get the hardware MAC address
pub fn get_mac_address() -> [u8; 6] {
    Efuse::mac_address()
}

/// Get the MAC address as a formatted string: "00:11:22:33:44:55" (Uppercased)
pub fn get_mac_address_str() -> heapless::String<17> {
    let mac = get_mac_address();
    let mut s = heapless::String::<17>::new();
    let _ = write!(
        s,
        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    );
    s
}

/// Get the device ID (MAC address without colons, lowercase)
pub fn get_device_id() -> heapless::String<12> {
    let mac = get_mac_address();
    let mut s = heapless::String::<12>::new();
    let _ = write!(
        s,
        "{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    );
    s
}

/// Get the chip ID as a u32 (24-bit value from last 3 bytes of MAC).
/// This intends to mirror https://github.com/espressif/arduino-esp32/blob/master/libraries/ESP32/examples/ChipID/GetChipID/GetChipID.ino
/// which is a smaller representation of the MAC to match ESP8266 behavior.
pub fn get_chip_id_u32() -> u32 {
    let mac = get_mac_address();
    // Arduino-equivalent logic:
    // i=0: (mac >> 40) & 0xff -> mac[5], chipId |= mac[5] << 0
    // i=8: (mac >> 32) & 0xff -> mac[4], chipId |= mac[4] << 8
    // i=16: (mac >> 24) & 0xff -> mac[3], chipId |= mac[3] << 16
    (mac[5] as u32) | ((mac[4] as u32) << 8) | ((mac[3] as u32) << 16)
}

/// Get the chip ID as a string.
pub fn get_chip_id() -> heapless::String<10> {
    let id = get_chip_id_u32();
    let mut s = heapless::String::<10>::new();
    let _ = write!(s, "{}", id);
    s
}

#[derive(Clone)]
pub struct DeviceInfo {
    pub mac_address: heapless::String<17>,
    pub chip_id: heapless::String<10>,
}

impl DeviceInfo {
    pub fn get() -> Self {
        Self {
            mac_address: get_mac_address_str(),
            chip_id: get_chip_id(),
        }
    }
}

pub fn resolve_reset_reason(
    reset_reason: Option<esp_hal::rtc_cntl::SocResetReason>,
) -> &'static str {
    match reset_reason {
        Some(esp_hal::rtc_cntl::SocResetReason::ChipPowerOn) => "ChipPowerOn",
        Some(esp_hal::rtc_cntl::SocResetReason::CoreEfuseCrc) => "CoreEfuseCrc",
        Some(esp_hal::rtc_cntl::SocResetReason::CoreMwdt0) => "CoreMwdt0",
        Some(esp_hal::rtc_cntl::SocResetReason::CoreMwdt1) => "CoreMwdt1",
        Some(esp_hal::rtc_cntl::SocResetReason::CoreRtcWdt) => "CoreRtcWdt",
        Some(esp_hal::rtc_cntl::SocResetReason::CoreSw) => "CoreSw",
        Some(esp_hal::rtc_cntl::SocResetReason::SysBrownOut) => "SysBrownOut",
        Some(esp_hal::rtc_cntl::SocResetReason::SysSuperWdt) => "SysSuperWdt",
        Some(esp_hal::rtc_cntl::SocResetReason::SysClkGlitch) => "SysClkGlitch",
        Some(esp_hal::rtc_cntl::SocResetReason::SysRtcWdt) => "SysRtcWdt",
        Some(esp_hal::rtc_cntl::SocResetReason::CoreUsbUart) => "CoreUsbUart",
        Some(esp_hal::rtc_cntl::SocResetReason::CoreUsbJtag) => "CoreUsbJtag",
        Some(esp_hal::rtc_cntl::SocResetReason::CorePwrGlitch) => "CorePwrGlitch",
        Some(esp_hal::rtc_cntl::SocResetReason::CoreDeepSleep) => "CoreDeepSleep",
        Some(esp_hal::rtc_cntl::SocResetReason::Cpu0Mwdt0) => "Cpu0Mwdt0",
        Some(esp_hal::rtc_cntl::SocResetReason::Cpu0Mwdt1) => "Cpu0Mwdt1",
        Some(esp_hal::rtc_cntl::SocResetReason::Cpu0Sw) => "Cpu0Sw",
        Some(esp_hal::rtc_cntl::SocResetReason::Cpu0RtcWdt) => "Cpu0RtcWdt",
        None => "None",
    }
}
