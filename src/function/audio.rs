//! USB Audio Class 2 (UAC2) function.
//!
//! The Linux kernel configuration option `CONFIG_USB_CONFIGFS_F_UAC2` must be enabled.
//!
//! # Example
//!
//! ```no_run
//! use usb_gadget::function::audio::{Uac2, Channel};
//! use usb_gadget::{default_udc, Class, Config, Gadget, Id, Strings};
//!
//! // capture: 8 ch, 48000 Hz, 24 bit, playback: 2 ch, 48000 Hz, 16 bit
//! let (audio, func) = Uac2::new(Channel::new(0b1111_1111, 48000, 24 / 8), Channel::new(0b11, 48000, 16 / 8)).build();
//!
//! let udc = default_udc().expect("cannot get UDC");
//! let reg =
//!     // USB device descriptor base class 0, 0, 0: use Interface Descriptors
//!     // Linux Foundation VID Gadget PID
//!     Gadget::new(Class::new(0, 0, 0), Id::new(0x1d6b, 0x0104), Strings::new("Clippy Manufacturer", "Rust UAC2", "RUST0123456"))
//!         .with_config(Config::new("Audio Config 1").with_function(func))
//!         .bind(&udc)
//!         .expect("cannot bind to UDC");
//!
//! println!(
//!     "UAC2 audio {} at {} to {} status {:?}",
//!     reg.name().to_string_lossy(),
//!     reg.path().display(),
//!     udc.name().to_string_lossy(),
//!     audio.status()
//! );
//! ```

use std::{ffi::OsString, io::Result};

use super::{
    util::{FunctionDir, Status},
    Function, Handle,
};

/// Audio channel configuration.
#[derive(Debug, Clone, Default)]
pub struct Channel {
    /// Audio channel mask. Set to 0 to disable the audio endpoint.
    ///
    /// The audio channel mask is a bit mask of the audio channels. The mask is a 32-bit integer with each bit representing a channel. The least significant bit is channel 1. The mask is used to specify the audio channels that are present in the audio stream. For example, a stereo stream would have a mask of 0x3 (channel 1 and channel 2).
    pub channel_mask: Option<u32>,
    /// Audio sample rate (Hz)
    pub sample_rate: Option<u32>,
    /// Audio sample size (bytes) so 2 bytes per sample (16 bit) would be 2.
    pub sample_size: Option<u32>,
}

impl Channel {
    /// Creates a new audio channel with the specified channel mask, sample rate (Hz), and sample size (bytes).
    pub fn new(channel_mask: u32, sample_rate: u32, sample_size: u32) -> Self {
        Self { channel_mask: Some(channel_mask), sample_rate: Some(sample_rate), sample_size: Some(sample_size) }
    }
}

/// Audio device configuration.
///
/// Fields are optional and will be set to f_uac2 default values if not specified, see drivers/usb/gadget/function/u_uac2.h. Not all fields are supported by all kernels; permission denied errors may occur if unsupported fields are set.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct Uac2Config {
    /// Audio channel configuration.
    pub channel: Channel,
    /// Audio sync type (capture only)
    pub sync_type: Option<u32>,
    /// Capture bInterval for HS/SS (1-4: fixed, 0: auto)
    pub hs_interval: Option<u8>,
    /// If channel has mute
    pub mute_present: Option<bool>,
    /// Terminal type
    pub terminal_type: Option<u8>,
    /// If channel has volume
    pub volume_present: Option<bool>,
    /// Minimum volume (in 1/256 dB)
    pub volume_min: Option<i16>,
    /// Maximum volume (in 1/256 dB)
    pub volume_max: Option<i16>,
    /// Resolution of volume control (in 1/256 dB)
    pub volume_resolution: Option<i16>,
    /// Name of the volume control function
    pub volume_name: Option<String>,
    /// Name of the input terminal
    pub input_terminal_name: Option<String>,
    /// Name of the input terminal channel
    pub input_terminal_channel_name: Option<String>,
    /// Name of the output terminal
    pub output_terminal_name: Option<String>,
}

/// Builder for USB audio class 2 (UAC2) function.
///
/// Set capture or playback channel_mask to 0 to disable the audio endpoint.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct Uac2Builder {
    /// Audio capture configuration.
    pub capture: Uac2Config,
    /// Audio playback configuration.
    pub playback: Uac2Config,
    /// Maximum extra bandwidth in async mode
    pub fb_max: Option<u32>,
    /// The number of pre-allocated request for both capture and playback
    pub request_number: Option<u32>,
    /// The name of the interface
    pub function_name: Option<String>,
    /// Topology control name
    pub control_name: Option<String>,
    /// The name of the input clock source
    pub clock_source_in_name: Option<String>,
    /// The name of the output clock source
    pub clock_source_out_name: Option<String>,
}

impl Uac2Builder {
    /// Build the USB function.
    ///
    /// The returned handle must be added to a USB gadget configuration.
    pub fn build(self) -> (Uac2, Handle) {
        let dir = FunctionDir::new();
        (Uac2 { dir: dir.clone() }, Handle::new(Uac2Function { builder: self, dir }))
    }

    /// Set audio capture configuration.
    #[must_use]
    pub fn with_capture_config(mut self, capture: Uac2Config) -> Self {
        self.capture = capture;
        self
    }

    /// Set audio playback configuration.
    #[must_use]
    pub fn with_playback_config(mut self, playback: Uac2Config) -> Self {
        self.playback = playback;
        self
    }
}

#[derive(Debug)]
struct Uac2Function {
    builder: Uac2Builder,
    dir: FunctionDir,
}

impl Function for Uac2Function {
    fn driver(&self) -> OsString {
        "uac2".into()
    }

    fn dir(&self) -> FunctionDir {
        self.dir.clone()
    }

    fn register(&self) -> Result<()> {
        // capture
        if let Some(channel_mask) = self.builder.capture.channel.channel_mask {
            self.dir.write("c_chmask", channel_mask.to_string())?;
        }
        if let Some(sample_rate) = self.builder.capture.channel.sample_rate {
            self.dir.write("c_srate", sample_rate.to_string())?;
        }
        if let Some(sample_size) = self.builder.capture.channel.sample_size {
            self.dir.write("c_ssize", sample_size.to_string())?;
        }
        if let Some(sync_type) = self.builder.capture.sync_type {
            self.dir.write("c_sync", sync_type.to_string())?;
        }
        if let Some(hs_interval) = self.builder.capture.hs_interval {
            self.dir.write("c_hs_bint", hs_interval.to_string())?;
        }
        if let Some(mute_present) = self.builder.capture.mute_present {
            self.dir.write("c_mute_present", (mute_present as u8).to_string())?;
        }
        if let Some(volume_present) = self.builder.capture.volume_present {
            self.dir.write("c_volume_present", (volume_present as u8).to_string())?;
        }
        if let Some(volume_min) = self.builder.capture.volume_min {
            self.dir.write("c_volume_min", volume_min.to_string())?;
        }
        if let Some(volume_max) = self.builder.capture.volume_max {
            self.dir.write("c_volume_max", volume_max.to_string())?;
        }
        if let Some(volume_resolution) = self.builder.capture.volume_resolution {
            self.dir.write("c_volume_res", volume_resolution.to_string())?;
        }
        if let Some(volume_name) = &self.builder.capture.volume_name {
            self.dir.write("c_fu_vol_name", volume_name)?;
        }
        if let Some(terminal_type) = self.builder.capture.terminal_type {
            self.dir.write("c_terminal_type", terminal_type.to_string())?;
        }
        if let Some(input_terminal_name) = &self.builder.capture.input_terminal_name {
            self.dir.write("c_it_name", input_terminal_name)?;
        }
        if let Some(input_terminal_channel_name) = &self.builder.capture.input_terminal_channel_name {
            self.dir.write("c_it_ch_name", input_terminal_channel_name)?;
        }
        if let Some(output_terminal_name) = &self.builder.capture.output_terminal_name {
            self.dir.write("c_ot_name", output_terminal_name)?;
        }

        // playback
        if let Some(channel_mask) = self.builder.playback.channel.channel_mask {
            self.dir.write("p_chmask", channel_mask.to_string())?;
        }
        if let Some(sample_rate) = self.builder.playback.channel.sample_rate {
            self.dir.write("p_srate", sample_rate.to_string())?;
        }
        if let Some(sample_size) = self.builder.playback.channel.sample_size {
            self.dir.write("p_ssize", sample_size.to_string())?;
        }
        if let Some(hs_interval) = self.builder.playback.hs_interval {
            self.dir.write("p_hs_bint", hs_interval.to_string())?;
        }
        if let Some(mute_present) = self.builder.playback.mute_present {
            self.dir.write("p_mute_present", (mute_present as u8).to_string())?;
        }
        if let Some(volume_present) = self.builder.playback.volume_present {
            self.dir.write("p_volume_present", (volume_present as u8).to_string())?;
        }
        if let Some(volume_min) = self.builder.playback.volume_min {
            self.dir.write("p_volume_min", volume_min.to_string())?;
        }
        if let Some(volume_max) = self.builder.playback.volume_max {
            self.dir.write("p_volume_max", volume_max.to_string())?;
        }
        if let Some(volume_resolution) = self.builder.playback.volume_resolution {
            self.dir.write("p_volume_res", volume_resolution.to_string())?;
        }
        if let Some(volume_name) = &self.builder.playback.volume_name {
            self.dir.write("p_fu_vol_name", volume_name)?;
        }
        if let Some(terminal_type) = self.builder.playback.terminal_type {
            self.dir.write("p_terminal_type", terminal_type.to_string())?;
        }
        if let Some(input_terminal_name) = &self.builder.playback.input_terminal_name {
            self.dir.write("p_it_name", input_terminal_name)?;
        }
        if let Some(input_terminal_channel_name) = &self.builder.playback.input_terminal_channel_name {
            self.dir.write("p_it_ch_name", input_terminal_channel_name)?;
        }
        if let Some(output_terminal_name) = &self.builder.playback.output_terminal_name {
            self.dir.write("p_ot_name", output_terminal_name)?;
        }

        // general
        if let Some(fb_max) = self.builder.fb_max {
            self.dir.write("fb_max", fb_max.to_string())?;
        }
        if let Some(request_number) = self.builder.request_number {
            self.dir.write("req_number", request_number.to_string())?;
        }
        if let Some(function_name) = &self.builder.function_name {
            self.dir.write("function_name", function_name)?;
        }
        if let Some(control_name) = &self.builder.control_name {
            self.dir.write("if_ctrl_name", control_name)?;
        }
        if let Some(clock_source_in_name) = &self.builder.clock_source_in_name {
            self.dir.write("clksrc_in_name", clock_source_in_name)?;
        }
        if let Some(clock_source_out_name) = &self.builder.clock_source_out_name {
            self.dir.write("clksrc_out_name", clock_source_out_name)?;
        }

        Ok(())
    }
}

/// USB Audio Class 2 (UAC2) function.
#[derive(Debug)]
pub struct Uac2 {
    dir: FunctionDir,
}

impl Uac2 {
    /// Creates a new USB Audio Class 2 (UAC2) builder with g_uac2 audio defaults.
    pub fn builder() -> Uac2Builder {
        Uac2Builder::default()
    }

    /// Creates a new USB Audio Class 2 (UAC2) function with the specified capture and playback channels.
    pub fn new(capture: Channel, playback: Channel) -> Uac2Builder {
        let mut builder = Uac2Builder::default();
        builder.capture.channel = capture;
        builder.playback.channel = playback;
        builder
    }

    /// Access to registration status.
    pub fn status(&self) -> Status {
        self.dir.status()
    }
}
