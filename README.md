# AirGradient Open Air

Hobbyist firmware for the [AirGradient Open Air (Model O-1PST)](https://www.airgradient.com/outdoor/).

This is **NOT the official firmware**; see [airgradient.com](https://www.airgradient.com/documentation/firmwares/) for that.

> ⚠️ **DISCLAIMER**: This firmware is provided as-is with no support. Use at your own risk. The author makes no guarantees about reliability, safety, or fitness for any purpose.

This project was written with significant assistance from LLMs. It likely has some bugs.

This has a minimal feature set:
- OpenMetrics-compatible endpoint at `http://<device-ip>/metrics`.
- Hardware watchdog integration.

## Watchdog
The watchdog is configured to check:
- WiFi connection, including DHCP assignment
- Sensor data freshness
- Metrics scrape recency

If any of those checks fail for long enough, the device will reset. See [`src/config.rs`](src/config.rs) for what those timeouts are.

## Metrics

### Device Info
| Metric | Description | Labels |
|--------|-------------|--------|
| `airgradient_info` | Device information | `version`, `commit`, `build_type`, `airgradient_serial_number`, `mac_address`, `reset_reason` |

### System Metrics
| Metric | Unit | Description |
|--------|------|-------------|
| `esp32_uptime_seconds` | seconds | System uptime since boot |
| `esp32_heap_used_bytes` | bytes | Currently used heap memory |
| `esp32_heap_total_bytes` | bytes | Total available heap memory |

### Air Quality Metrics
| Metric | Unit | Description |
|--------|------|-------------|
| `airgradient_pm0d3_p100ml` | particles/100ml | PM0.3 particle count |
| `airgradient_pm0d5_p100ml` | particles/100ml | PM0.5 particle count |
| `airgradient_pm1_p100ml` | particles/100ml | PM1.0 particle count |
| `airgradient_pm2d5_p100ml` | particles/100ml | PM2.5 particle count |
| `airgradient_pm1_ugm3` | µg/m³ | PM1.0 concentration |
| `airgradient_pm2d5_ugm3` | µg/m³ | PM2.5 concentration |
| `airgradient_pm10_ugm3` | µg/m³ | PM10 concentration |
| `airgradient_co2_ppm` | ppm | CO2 concentration |
| `airgradient_tvoc_index` | index (1-500) | [TVOC index](https://sensirion.github.io/gas-index-algorithm/) |
| `airgradient_nox_index` | index (1-500) | [NOx index](https://sensirion.github.io/gas-index-algorithm/) |
| `airgradient_temperature_celsius` | °C | Temperature |
| `airgradient_humidity_percent` | % | Relative humidity |

### Error Metrics
| Metric | Labels | Description |
|--------|--------|-------------|
| `airgradient_sensor_error` | `sensor`, `error` | Per-sensor error status (0 = OK, 1 = error) |

## Building

### Prerequisites

Follow the [Espressif Rust Book](https://docs.espressif.com/projects/rust/book/getting-started/toolchain.html) to set up the Rust toolchain for ESP32 development.

This uses a nightly toolchain.

### Build and Flash

```bash
env DEFMT_LOG=info WIFI_SSID="SomeNetwork" WIFI_PASSWORD="SomePassword" cargo run --release
```

This uses [defmt](https://defmt.ferrous-systems.com/) for logging. See the [filtering](https://defmt.ferrous-systems.com/filtering) doc section for available options.

### Configuration

Most configuration is in [`src/config.rs`](src/config.rs).

# Credits
[Rust on ESP Book](https://docs.espressif.com/projects/rust/book/).
[impl Rust for ESP32](https://esp32.implrust.com/). Great resource with examples on how to get started with Embassy async. I borrowed most of the wifi/web parts.
