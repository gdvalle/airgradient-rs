use crate::{device::DeviceInfo, sensors::SharedSensorData};
use core::fmt::{self, Write as FmtWrite};
use core::sync::atomic::{AtomicU32, Ordering};
use embassy_time::Instant;
use picoserve::response::{Content, IntoResponse, StatusCode};

extern crate alloc;
use alloc::string::String;

#[derive(Debug, Clone, Copy)]
pub struct SystemMetrics {
    pub heap_bytes_used: usize,
    pub heap_bytes_total: usize,
}

impl SystemMetrics {
    pub fn capture() -> Self {
        let heap_stats = esp_alloc::HEAP.stats();
        Self {
            heap_bytes_used: heap_stats.current_usage,
            heap_bytes_total: heap_stats.size,
        }
    }
}

pub struct MetricsContent(pub String);
enum MetricsResponse {
    Metrics(MetricsContent),
    Error(&'static str),
}

impl Content for MetricsResponse {
    fn content_type(&self) -> &'static str {
        match self {
            MetricsResponse::Metrics(_) => {
                "application/openmetrics-text; version=1.0.0; charset=utf-8"
            }
            MetricsResponse::Error(_) => "text/plain; charset=utf-8",
        }
    }

    fn content_length(&self) -> usize {
        match self {
            MetricsResponse::Metrics(metrics) => metrics.0.len(),
            MetricsResponse::Error(error) => error.len(),
        }
    }

    async fn write_content<W: picoserve::io::Write>(self, mut writer: W) -> Result<(), W::Error> {
        match self {
            MetricsResponse::Metrics(metrics) => writer.write_all(metrics.0.as_bytes()).await,
            MetricsResponse::Error(error) => writer.write_all(error.as_bytes()).await,
        }
    }
}

/// Helper to write Prometheus format into a generic fmt::Write (like String)
struct MetricFormatter<'a, W: FmtWrite> {
    writer: &'a mut W,
}

impl<'a, W: FmtWrite> MetricFormatter<'a, W> {
    fn new(writer: &'a mut W) -> Self {
        Self { writer }
    }

    fn write_gauge(
        &mut self,
        name: &str,
        help: &str,
        unit: Option<&str>,
        value: impl fmt::Display,
        labels: Option<&str>,
    ) -> fmt::Result {
        writeln!(self.writer, "# HELP {} {}", name, help)?;
        writeln!(self.writer, "# TYPE {} gauge", name)?;
        if let Some(u) = unit {
            writeln!(self.writer, "# UNIT {} {}", name, u)?;
        }

        write!(self.writer, "{}", name)?;
        if let Some(lbl) = labels {
            write!(self.writer, "{{{}}}", lbl)?;
        } else {
            write!(self.writer, "{{}}")?;
        }
        writeln!(self.writer, " {}", value)?;
        Ok(())
    }
}

pub async fn metrics_handler(
    shared_sensor_data: SharedSensorData,
    device_info: DeviceInfo,
    reset_reason: &'static str,
    last_scrape_secs: &AtomicU32,
) -> impl IntoResponse {
    let now = Instant::now();
    let now_secs = now.as_secs();

    let sensor_data = {
        let lock = shared_sensor_data.lock().await;
        lock.clone()
    };

    if !sensor_data.initialized {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            MetricsResponse::Error("Sensors are initializing\n"),
        );
    }

    // Pre-allocate a reasonable chunk of memory to avoid multiple re-allocations.
    // TODO: A test to be sure this isn't too small?
    let mut output = String::with_capacity(2048);
    let mut mf = MetricFormatter::new(&mut output);

    let version = env!("CARGO_PKG_VERSION");
    let commit = option_env!("GIT_HASH").unwrap_or("unknown");
    let build_type = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };

    let labels = {
        let mut lb = String::new();
        let _ = write!(
            lb,
            "version=\"{}\",commit=\"{}\",build_type=\"{}\",airgradient_serial_number=\"{}\",mac_address=\"{}\",reset_reason=\"{}\"",
            version,
            commit,
            build_type,
            &device_info.chip_id,
            &device_info.mac_address,
            reset_reason
        );
        lb
    };

    let _ = mf.write_gauge(
        "airgradient_info",
        "Device info",
        Some("info"),
        1,
        Some(&labels),
    );

    // System Metrics
    let sys = SystemMetrics::capture();
    let _ = mf.write_gauge(
        "esp32_uptime_seconds",
        "System uptime",
        Some("seconds"),
        now_secs,
        None,
    );
    let _ = mf.write_gauge(
        "esp32_heap_used_bytes",
        "Used heap memory",
        Some("bytes"),
        sys.heap_bytes_used,
        None,
    );
    let _ = mf.write_gauge(
        "esp32_heap_total_bytes",
        "Total heap memory",
        Some("bytes"),
        sys.heap_bytes_total,
        None,
    );

    // Sensor Data
    let s = &sensor_data;
    let _ = mf.write_gauge(
        "airgradient_pm0d3_p100ml",
        "PM0.3",
        Some("p100ml"),
        s.pm03_count,
        None,
    );
    let _ = mf.write_gauge(
        "airgradient_pm0d5_p100ml",
        "PM0.5",
        Some("p100ml"),
        s.pm05_count,
        None,
    );
    let _ = mf.write_gauge(
        "airgradient_pm1_p100ml",
        "PM1.0 count",
        Some("p100ml"),
        s.pm10_count,
        None,
    );
    let _ = mf.write_gauge(
        "airgradient_pm2d5_p100ml",
        "PM2.5 count",
        Some("p100ml"),
        s.pm25_count,
        None,
    );
    let _ = mf.write_gauge("airgradient_pm1_ugm3", "PM1.0", Some("ugm3"), s.pm1, None);
    let _ = mf.write_gauge(
        "airgradient_pm2d5_ugm3",
        "PM2.5",
        Some("ugm3"),
        s.pm25,
        None,
    );
    let _ = mf.write_gauge("airgradient_pm10_ugm3", "PM10", Some("ugm3"), s.pm10, None);
    let _ = mf.write_gauge("airgradient_co2_ppm", "CO2", Some("ppm"), s.co2, None);

    let _ = mf.write_gauge("airgradient_tvoc_index", "TVOC", Some("index"), s.voc, None);
    let _ = mf.write_gauge("airgradient_nox_index", "NOx", Some("index"), s.nox, None);

    let _ = mf.write_gauge(
        "airgradient_temperature_celsius",
        "Temp C",
        Some("celsius"),
        s.temp,
        None,
    );
    let _ = mf.write_gauge(
        "airgradient_humidity_percent",
        "Humidity",
        Some("percent"),
        s.humidity,
        None,
    );

    // Sensor errors. Record one a gauge with a label for each sensor type.
    // If an error is present, we include error="VariantName".
    let mut report_error = |name: &str, err: Option<&dyn core::fmt::Debug>| {
        let mut lbl: heapless::String<96> = heapless::String::new();
        // Base label
        let _ = write!(lbl, "sensor=\"{}\"", name);
        let val = if let Some(e) = err {
            // Get debug string
            let mut dbg_str: heapless::String<48> = heapless::String::new();
            let _ = write!(dbg_str, "{:?}", e);
            // clean it (remove paren data if any, e.g. "SomeError(123)" -> "SomeError")
            let variant = dbg_str.split('(').next().unwrap_or(&dbg_str);
            let _ = write!(lbl, ",error=\"{}\"", variant);
            1
        } else {
            // For label discovery purposes, output an empty label.
            let _ = write!(lbl, ",error=\"\"");
            0
        };
        let _ = mf.write_gauge(
            "airgradient_sensor_error",
            "Sensor Error Status",
            None,
            val,
            Some(&lbl),
        );
    };

    let errs = s.errors.as_ref();
    report_error(
        "pms",
        errs.and_then(|x| x.pms.as_ref())
            .map(|e| e as &dyn core::fmt::Debug),
    );
    report_error(
        "sgp",
        errs.and_then(|x| x.sgp.as_ref())
            .map(|e| e as &dyn core::fmt::Debug),
    );
    report_error(
        "s8",
        errs.and_then(|x| x.s8.as_ref())
            .map(|e| e as &dyn core::fmt::Debug),
    );

    let _ = writeln!(output, "# EOF");

    last_scrape_secs.store(now_secs as u32, Ordering::Relaxed);

    (
        StatusCode::OK,
        MetricsResponse::Metrics(MetricsContent(output)),
    )
}
