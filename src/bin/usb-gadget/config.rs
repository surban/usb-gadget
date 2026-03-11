//! TOML configuration file types.

use serde::Deserialize;
use std::collections::HashMap;

/// Top-level gadget configuration.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GadgetConfig {
    /// Gadget name in configfs (defaults to filename stem).
    pub name: Option<String>,
    /// USB device controller name (auto-detect if omitted).
    pub udc: Option<String>,
    /// Device descriptor fields.
    pub device: DeviceConfig,
    /// OS descriptor.
    pub os_descriptor: Option<OsDescriptorConfig>,
    /// WebUSB descriptor.
    pub web_usb: Option<WebUsbConfig>,
    /// USB configurations (at least one required).
    pub config: Vec<UsbConfigConfig>,
}

/// Device-level configuration.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceConfig {
    /// Vendor ID.
    pub vendor: u16,
    /// Product ID.
    pub product: u16,
    /// Device class code (default: 0 = interface-specific).
    pub class: Option<u8>,
    /// Device sub-class code.
    pub sub_class: Option<u8>,
    /// Device protocol code.
    pub protocol: Option<u8>,
    /// Manufacturer name.
    pub manufacturer: Option<String>,
    /// Product name.
    pub product_name: Option<String>,
    /// Serial number.
    pub serial: Option<String>,
    /// Language for the strings above (default: "en-us").
    pub language: Option<String>,
    /// Maximum endpoint 0 packet size.
    pub max_packet_size0: Option<u8>,
    /// Device release number (BCD).
    pub device_release: Option<u16>,
    /// USB specification version.
    pub usb_version: Option<String>,
    /// Maximum USB speed.
    pub max_speed: Option<String>,
}

/// OS descriptor configuration.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OsDescriptorConfig {
    pub vendor_code: u8,
    pub qw_sign: String,
    pub config: Option<usize>,
}

/// WebUSB configuration.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebUsbConfig {
    pub vendor_code: u8,
    pub landing_page: String,
    pub version: Option<String>,
}

/// A USB configuration.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsbConfigConfig {
    /// Configuration description.
    pub description: Option<String>,
    /// Maximum power in mA.
    pub max_power: Option<u16>,
    /// Self-powered flag.
    pub self_powered: Option<bool>,
    /// Remote wakeup flag.
    pub remote_wakeup: Option<bool>,
    /// Functions in this configuration.
    pub function: Vec<FunctionConfig>,
}

/// A USB function specification.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
#[serde(rename_all = "snake_case")]
pub enum FunctionConfig {
    Serial(SerialConfig),
    Hid(HidConfig),
    Net(NetConfig),
    Uac2(Uac2FnConfig),
    Uac1(Uac1FnConfig),
    Midi(MidiConfig),
    Msd(MsdConfig),
    Printer(PrinterConfig),
    Loopback(LoopbackConfig),
    SourceSink(SourceSinkConfig),
    Video(VideoConfig),
    Other(OtherConfig),
}

// --- Per-function configs ---

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SerialConfig {
    /// "acm" or "generic".
    pub class: String,
    pub console: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HidConfig {
    pub sub_class: Option<u8>,
    pub protocol: Option<u8>,
    /// Report descriptor as hex string, e.g. "05010906...".
    pub report_desc: Option<String>,
    pub report_len: Option<u8>,
    pub no_out_endpoint: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetConfig {
    /// "ecm", "ecm_subset", "eem", "ncm", or "rndis".
    pub class: String,
    /// Device MAC address.
    pub dev_addr: Option<String>,
    /// Host MAC address.
    pub host_addr: Option<String>,
    pub qmult: Option<u32>,
    /// RNDIS interface class override.
    pub interface_class: Option<u8>,
    pub interface_sub_class: Option<u8>,
    pub interface_protocol: Option<u8>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelConfig {
    pub channel_mask: Option<u32>,
    pub sample_rate: Option<u32>,
    pub sample_size: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Uac2EndpointConfig {
    pub channel: Option<ChannelConfig>,
    pub sync_type: Option<u32>,
    pub hs_interval: Option<u8>,
    pub mute_present: Option<bool>,
    pub terminal_type: Option<u8>,
    pub volume_present: Option<bool>,
    pub volume_min: Option<i16>,
    pub volume_max: Option<i16>,
    pub volume_resolution: Option<i16>,
    pub volume_name: Option<String>,
    pub input_terminal_name: Option<String>,
    pub input_terminal_channel_name: Option<String>,
    pub output_terminal_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Uac2FnConfig {
    pub capture: Option<Uac2EndpointConfig>,
    pub playback: Option<Uac2EndpointConfig>,
    pub fb_max: Option<u32>,
    pub request_number: Option<u32>,
    pub function_name: Option<String>,
    pub control_name: Option<String>,
    pub clock_source_in_name: Option<String>,
    pub clock_source_out_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Uac1EndpointConfig {
    pub channel: Option<ChannelConfig>,
    pub mute_present: Option<bool>,
    pub volume_present: Option<bool>,
    pub volume_min: Option<i16>,
    pub volume_max: Option<i16>,
    pub volume_resolution: Option<i16>,
    pub volume_name: Option<String>,
    pub input_terminal_name: Option<String>,
    pub input_terminal_channel_name: Option<String>,
    pub output_terminal_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Uac1FnConfig {
    pub capture: Option<Uac1EndpointConfig>,
    pub playback: Option<Uac1EndpointConfig>,
    pub request_number: Option<u32>,
    pub function_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MidiConfig {
    pub buflen: Option<u16>,
    pub id: Option<String>,
    pub in_ports: Option<u8>,
    pub out_ports: Option<u8>,
    pub index: Option<u8>,
    pub qlen: Option<u8>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LunConfig {
    pub file: Option<String>,
    pub read_only: Option<bool>,
    pub cdrom: Option<bool>,
    pub no_fua: Option<bool>,
    pub removable: Option<bool>,
    pub inquiry_string: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MsdConfig {
    pub stall: Option<bool>,
    pub lun: Vec<LunConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrinterConfig {
    pub pnp_string: Option<String>,
    pub qlen: Option<u8>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoopbackConfig {
    pub qlen: Option<u32>,
    pub bulk_buflen: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSinkConfig {
    pub pattern: Option<u32>,
    pub isoc_interval: Option<u32>,
    pub isoc_maxpacket: Option<u32>,
    pub isoc_mult: Option<u32>,
    pub isoc_maxburst: Option<u32>,
    pub bulk_buflen: Option<u32>,
    pub bulk_qlen: Option<u32>,
    pub iso_qlen: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameConfig {
    pub width: u32,
    pub height: u32,
    /// "yuyv", "mjpeg", "nv12", "h264", or a custom GUID hex string.
    pub format: String,
    /// Frame rates (e.g. [30, 60]).
    pub fps: Option<Vec<u16>>,
    /// Raw intervals in 100ns units (alternative to fps).
    pub intervals: Option<Vec<u32>>,
    pub color_matching: Option<ColorMatchingConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorMatchingConfig {
    pub color_primaries: Option<u8>,
    pub transfer_characteristics: Option<u8>,
    pub matrix_coefficients: Option<u8>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideoConfig {
    pub streaming_interval: Option<u8>,
    pub streaming_max_burst: Option<u8>,
    pub streaming_max_packet: Option<u32>,
    pub function_name: Option<String>,
    pub processing_controls: Option<u8>,
    pub camera_controls: Option<u8>,
    pub frame: Option<Vec<FrameConfig>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OtherConfig {
    /// Kernel function driver name.
    pub driver: String,
    /// Key-value properties.
    pub properties: Option<HashMap<String, String>>,
}
