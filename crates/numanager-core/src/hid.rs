use crate::{Error, ErrorCode, Result};
use std::collections::VecDeque;

pub trait HidFeatureIo: Send {
    fn set_feature(&mut self, report: &[u8]) -> Result<()>;
    fn get_feature(&mut self, report_id: u8, len: usize) -> Result<Vec<u8>>;
}

pub trait HidReportIo: Send {
    fn write_report(&mut self, report: &[u8]) -> Result<()>;
    fn read_report(&mut self, len: usize) -> Result<Vec<u8>>;
}

#[derive(Debug, Clone)]
pub struct HidDeviceIdentity {
    pub vendor_id: u16,
    pub product_id: u16,
    pub product_string: Option<String>,
    pub serial_number: Option<String>,
}

#[cfg(feature = "os-hid")]
#[derive(Debug, Clone)]
pub struct OsHidFeatureConfig {
    pub vendor_id: u16,
    pub product_id: u16,
    pub serial_number: Option<String>,
    pub read_timeout_ms: i32,
}

#[cfg(feature = "os-hid")]
impl OsHidFeatureConfig {
    pub fn new(vendor_id: u16, product_id: u16) -> Self {
        Self {
            vendor_id,
            product_id,
            serial_number: None,
            read_timeout_ms: 100,
        }
    }

    pub fn serial_number(mut self, serial_number: impl Into<String>) -> Self {
        self.serial_number = Some(serial_number.into());
        self
    }

    pub fn read_timeout_ms(mut self, timeout_ms: i32) -> Self {
        self.read_timeout_ms = timeout_ms.max(0);
        self
    }
}

#[cfg(feature = "os-hid")]
pub struct OsHidFeatureDevice {
    device: hidapi::HidDevice,
}

#[cfg(feature = "os-hid")]
impl OsHidFeatureDevice {
    pub fn open(vendor_id: u16, product_id: u16) -> Result<Self> {
        Self::open_config(OsHidFeatureConfig::new(vendor_id, product_id))
    }

    pub fn open_config(config: OsHidFeatureConfig) -> Result<Self> {
        let api = hidapi::HidApi::new().map_err(map_hid_error)?;
        let device = match &config.serial_number {
            Some(serial) => api.open_serial(config.vendor_id, config.product_id, serial),
            None => api.open(config.vendor_id, config.product_id),
        }
        .map_err(map_hid_error)?;
        Ok(Self { device })
    }
}

#[cfg(feature = "os-hid")]
impl HidFeatureIo for OsHidFeatureDevice {
    fn set_feature(&mut self, report: &[u8]) -> Result<()> {
        self.device
            .send_feature_report(report)
            .map(|_| ())
            .map_err(map_hid_error)
    }

    fn get_feature(&mut self, report_id: u8, len: usize) -> Result<Vec<u8>> {
        if len == 0 {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "HID feature report length must be nonzero",
            ));
        }
        let mut report = vec![0; len];
        report[0] = report_id;
        let n = self
            .device
            .get_feature_report(report.as_mut_slice())
            .map_err(map_hid_error)?;
        report.truncate(n.max(1));
        Ok(report)
    }
}

#[cfg(feature = "os-hid")]
pub struct OsHidReportDevice {
    device: hidapi::HidDevice,
    report_id: u8,
    read_timeout_ms: i32,
}

#[cfg(feature = "os-hid")]
impl OsHidReportDevice {
    pub fn open_config(config: OsHidFeatureConfig, report_id: u8) -> Result<Self> {
        let api = hidapi::HidApi::new().map_err(map_hid_error)?;
        let device = match &config.serial_number {
            Some(serial) => api.open_serial(config.vendor_id, config.product_id, serial),
            None => api.open(config.vendor_id, config.product_id),
        }
        .map_err(map_hid_error)?;
        Ok(Self {
            device,
            report_id,
            read_timeout_ms: config.read_timeout_ms,
        })
    }
}

#[cfg(feature = "os-hid")]
impl HidReportIo for OsHidReportDevice {
    fn write_report(&mut self, report: &[u8]) -> Result<()> {
        let mut payload = Vec::with_capacity(report.len() + 1);
        payload.push(self.report_id);
        payload.extend(report);
        self.device
            .write(&payload)
            .map(|_| ())
            .map_err(map_hid_error)
    }

    fn read_report(&mut self, len: usize) -> Result<Vec<u8>> {
        if len == 0 {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "HID input report length must be nonzero",
            ));
        }
        let mut report = vec![0; len + 1];
        let n = self
            .device
            .read_timeout(report.as_mut_slice(), self.read_timeout_ms)
            .map_err(map_hid_error)?;
        report.truncate(n);
        if report.len() == len + 1 && report.first() == Some(&self.report_id) {
            report.remove(0);
        }
        report.resize(len, 0);
        Ok(report)
    }
}

#[cfg(feature = "os-hid")]
pub fn enumerate_hid_devices() -> Result<Vec<HidDeviceIdentity>> {
    let api = hidapi::HidApi::new().map_err(map_hid_error)?;
    Ok(api
        .device_list()
        .map(|device| HidDeviceIdentity {
            vendor_id: device.vendor_id(),
            product_id: device.product_id(),
            product_string: device.product_string().map(ToOwned::to_owned),
            serial_number: device.serial_number().map(ToOwned::to_owned),
        })
        .collect())
}

#[derive(Debug, Clone, Default)]
pub struct ScriptedHidReport {
    writes: Vec<Vec<u8>>,
    reads: VecDeque<Vec<u8>>,
}

impl ScriptedHidReport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_reads(reads: impl IntoIterator<Item = Vec<u8>>) -> Self {
        Self {
            writes: Vec::new(),
            reads: reads.into_iter().collect(),
        }
    }

    pub fn push_read(&mut self, report: impl Into<Vec<u8>>) {
        self.reads.push_back(report.into());
    }

    pub fn writes(&self) -> &[Vec<u8>] {
        &self.writes
    }
}

impl HidReportIo for ScriptedHidReport {
    fn write_report(&mut self, report: &[u8]) -> Result<()> {
        self.writes.push(report.to_vec());
        Ok(())
    }

    fn read_report(&mut self, len: usize) -> Result<Vec<u8>> {
        if len == 0 {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "HID input report length must be nonzero",
            ));
        }
        let mut report = self.reads.pop_front().unwrap_or_default();
        report.resize(len, 0);
        Ok(report)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScriptedHidFeature {
    writes: Vec<Vec<u8>>,
    reads: VecDeque<Vec<u8>>,
}

impl ScriptedHidFeature {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_reads(reads: impl IntoIterator<Item = Vec<u8>>) -> Self {
        Self {
            writes: Vec::new(),
            reads: reads.into_iter().collect(),
        }
    }

    pub fn push_read(&mut self, report: impl Into<Vec<u8>>) {
        self.reads.push_back(report.into());
    }

    pub fn writes(&self) -> &[Vec<u8>] {
        &self.writes
    }
}

impl HidFeatureIo for ScriptedHidFeature {
    fn set_feature(&mut self, report: &[u8]) -> Result<()> {
        self.writes.push(report.to_vec());
        Ok(())
    }

    fn get_feature(&mut self, report_id: u8, len: usize) -> Result<Vec<u8>> {
        if len == 0 {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "HID feature report length must be nonzero",
            ));
        }
        let mut report = self.reads.pop_front().unwrap_or_else(|| vec![report_id; 1]);
        if report.is_empty() {
            report.push(report_id);
        } else {
            report[0] = report_id;
        }
        report.resize(len, 0);
        Ok(report)
    }
}

#[cfg(feature = "os-hid")]
fn map_hid_error(error: hidapi::HidError) -> Error {
    Error::new(ErrorCode::Transport, error.to_string())
}
