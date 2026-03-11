//! Build usb-gadget types from config structs.

use std::{
    io::{Error, ErrorKind, Result},
    path::PathBuf,
};

use usb_gadget::{
    function::{self, Handle},
    Class, Config, Gadget, Id, Language, OsDescriptor, Speed, Strings, UsbVersion,
};

use crate::config::*;

/// Build a [`Gadget`] from a parsed config, returning the gadget and the name to use.
pub fn build_gadget(cfg: &GadgetConfig) -> Result<(Gadget, Option<String>)> {
    let device = &cfg.device;

    let class =
        Class::new(device.class.unwrap_or(0), device.sub_class.unwrap_or(0), device.protocol.unwrap_or(0));
    let id = Id::new(device.vendor, device.product);

    let lang = parse_language(device.language.as_deref().unwrap_or("en-us"))?;
    let strings = Strings::new(
        device.manufacturer.as_deref().unwrap_or(""),
        device.product_name.as_deref().unwrap_or(""),
        device.serial.as_deref().unwrap_or(""),
    );

    let mut gadget = Gadget::new(class, id, strings.clone());
    if lang != Language::default() {
        gadget.strings.clear();
        gadget.strings.insert(lang, strings);
    }

    if let Some(v) = device.max_packet_size0 {
        gadget.max_packet_size0 = v;
    }
    if let Some(v) = device.device_release {
        gadget.device_release = v;
    }
    if let Some(ref v) = device.usb_version {
        gadget.usb_version = parse_usb_version(v)?;
    }
    if let Some(ref v) = device.max_speed {
        gadget.max_speed = Some(parse_speed(v)?);
    }

    if let Some(ref os) = cfg.os_descriptor {
        gadget.os_descriptor = Some(OsDescriptor {
            vendor_code: os.vendor_code,
            qw_sign: os.qw_sign.clone(),
            config: os.config.unwrap_or(0),
        });
    }

    if let Some(ref web) = cfg.web_usb {
        let version = match web.version.as_deref() {
            Some("1.0") | None => usb_gadget::WebUsbVersion::V10,
            Some(other) => {
                let v = parse_hex_u16(other)?;
                usb_gadget::WebUsbVersion::Other(v)
            }
        };
        gadget.web_usb = Some(usb_gadget::WebUsb {
            version,
            vendor_code: web.vendor_code,
            landing_page: web.landing_page.clone(),
        });
    }

    if cfg.config.is_empty() {
        return Err(Error::new(ErrorKind::InvalidInput, "at least one [[config]] is required"));
    }

    for usb_cfg in &cfg.config {
        let mut config = Config::new(usb_cfg.description.as_deref().unwrap_or(""));
        if let Some(v) = usb_cfg.max_power {
            config.max_power = v;
        }
        if let Some(v) = usb_cfg.self_powered {
            config.self_powered = v;
        }
        if let Some(v) = usb_cfg.remote_wakeup {
            config.remote_wakeup = v;
        }

        for func_cfg in &usb_cfg.function {
            let handle = build_function(func_cfg)?;
            config.functions.push(handle);
        }

        gadget.configs.push(config);
    }

    Ok((gadget, cfg.name.clone()))
}

/// Build a function [`Handle`] from a function config.
fn build_function(cfg: &FunctionConfig) -> Result<Handle> {
    match cfg {
        FunctionConfig::Serial(c) => build_serial(c),
        FunctionConfig::Hid(c) => build_hid(c),
        FunctionConfig::Net(c) => build_net(c),
        FunctionConfig::Uac2(c) => build_uac2(c),
        FunctionConfig::Uac1(c) => build_uac1(c),
        FunctionConfig::Midi(c) => build_midi(c),
        FunctionConfig::Msd(c) => build_msd(c),
        FunctionConfig::Printer(c) => build_printer(c),
        FunctionConfig::Loopback(c) => build_loopback(c),
        FunctionConfig::SourceSink(c) => build_source_sink(c),
        FunctionConfig::Video(c) => build_video(c),
        FunctionConfig::Other(c) => build_other(c),
    }
}

fn build_serial(c: &SerialConfig) -> Result<Handle> {
    let class = match c.class.as_str() {
        "acm" => function::serial::SerialClass::Acm,
        "generic" => function::serial::SerialClass::Generic,
        other => return Err(Error::new(ErrorKind::InvalidInput, format!("unknown serial class: {other}"))),
    };
    let mut b = function::serial::Serial::builder(class);
    b.console = c.console;
    let (_serial, handle) = b.build();
    Ok(handle)
}

fn build_hid(c: &HidConfig) -> Result<Handle> {
    let mut b = function::hid::Hid::builder();
    if let Some(v) = c.sub_class {
        b.sub_class = v;
    }
    if let Some(v) = c.protocol {
        b.protocol = v;
    }
    if let Some(ref hex) = c.report_desc {
        b.report_desc = parse_hex_bytes(hex)?;
    }
    if let Some(v) = c.report_len {
        b.report_len = v;
    }
    if let Some(v) = c.no_out_endpoint {
        b.no_out_endpoint = v;
    }
    let (_hid, handle) = b.build();
    Ok(handle)
}

fn build_net(c: &NetConfig) -> Result<Handle> {
    let class = match c.class.as_str() {
        "ecm" => function::net::NetClass::Ecm,
        "ecm_subset" => function::net::NetClass::EcmSubset,
        "eem" => function::net::NetClass::Eem,
        "ncm" => function::net::NetClass::Ncm,
        "rndis" => function::net::NetClass::Rndis,
        other => return Err(Error::new(ErrorKind::InvalidInput, format!("unknown net class: {other}"))),
    };
    let mut b = function::net::Net::builder(class);
    if let Some(ref addr) = c.dev_addr {
        b.dev_addr =
            Some(addr.parse().map_err(|e| Error::new(ErrorKind::InvalidInput, format!("bad dev_addr: {e}")))?);
    }
    if let Some(ref addr) = c.host_addr {
        b.host_addr =
            Some(addr.parse().map_err(|e| Error::new(ErrorKind::InvalidInput, format!("bad host_addr: {e}")))?);
    }
    b.qmult = c.qmult;
    if c.interface_class.is_some() || c.interface_sub_class.is_some() || c.interface_protocol.is_some() {
        b.interface_class = Some(Class::new(
            c.interface_class.unwrap_or(0),
            c.interface_sub_class.unwrap_or(0),
            c.interface_protocol.unwrap_or(0),
        ));
    }
    let (_net, handle) = b.build();
    Ok(handle)
}

fn apply_channel(ch: &Option<ChannelConfig>) -> function::audio::Channel {
    let mut channel = function::audio::Channel::default();
    if let Some(ref c) = ch {
        channel.channel_mask = c.channel_mask;
        channel.sample_rate = c.sample_rate;
        channel.sample_size = c.sample_size;
    }
    channel
}

fn build_uac2(c: &Uac2FnConfig) -> Result<Handle> {
    let mut b = function::audio::Uac2::builder();

    if let Some(ref cap) = c.capture {
        b.capture.channel = apply_channel(&cap.channel);
        b.capture.sync_type = cap.sync_type;
        b.capture.hs_interval = cap.hs_interval;
        b.capture.mute_present = cap.mute_present;
        b.capture.terminal_type = cap.terminal_type;
        b.capture.volume_present = cap.volume_present;
        b.capture.volume_min = cap.volume_min;
        b.capture.volume_max = cap.volume_max;
        b.capture.volume_resolution = cap.volume_resolution;
        b.capture.volume_name = cap.volume_name.clone();
        b.capture.input_terminal_name = cap.input_terminal_name.clone();
        b.capture.input_terminal_channel_name = cap.input_terminal_channel_name.clone();
        b.capture.output_terminal_name = cap.output_terminal_name.clone();
    }

    if let Some(ref play) = c.playback {
        b.playback.channel = apply_channel(&play.channel);
        b.playback.sync_type = play.sync_type;
        b.playback.hs_interval = play.hs_interval;
        b.playback.mute_present = play.mute_present;
        b.playback.terminal_type = play.terminal_type;
        b.playback.volume_present = play.volume_present;
        b.playback.volume_min = play.volume_min;
        b.playback.volume_max = play.volume_max;
        b.playback.volume_resolution = play.volume_resolution;
        b.playback.volume_name = play.volume_name.clone();
        b.playback.input_terminal_name = play.input_terminal_name.clone();
        b.playback.input_terminal_channel_name = play.input_terminal_channel_name.clone();
        b.playback.output_terminal_name = play.output_terminal_name.clone();
    }

    b.fb_max = c.fb_max;
    b.request_number = c.request_number;
    b.function_name = c.function_name.clone();
    b.control_name = c.control_name.clone();
    b.clock_source_in_name = c.clock_source_in_name.clone();
    b.clock_source_out_name = c.clock_source_out_name.clone();

    let (_uac2, handle) = b.build();
    Ok(handle)
}

fn build_uac1(c: &Uac1FnConfig) -> Result<Handle> {
    let mut b = function::audio::Uac1::builder();

    if let Some(ref cap) = c.capture {
        b.capture.channel = apply_channel(&cap.channel);
        b.capture.mute_present = cap.mute_present;
        b.capture.volume_present = cap.volume_present;
        b.capture.volume_min = cap.volume_min;
        b.capture.volume_max = cap.volume_max;
        b.capture.volume_resolution = cap.volume_resolution;
        b.capture.volume_name = cap.volume_name.clone();
        b.capture.input_terminal_name = cap.input_terminal_name.clone();
        b.capture.input_terminal_channel_name = cap.input_terminal_channel_name.clone();
        b.capture.output_terminal_name = cap.output_terminal_name.clone();
    }

    if let Some(ref play) = c.playback {
        b.playback.channel = apply_channel(&play.channel);
        b.playback.mute_present = play.mute_present;
        b.playback.volume_present = play.volume_present;
        b.playback.volume_min = play.volume_min;
        b.playback.volume_max = play.volume_max;
        b.playback.volume_resolution = play.volume_resolution;
        b.playback.volume_name = play.volume_name.clone();
        b.playback.input_terminal_name = play.input_terminal_name.clone();
        b.playback.input_terminal_channel_name = play.input_terminal_channel_name.clone();
        b.playback.output_terminal_name = play.output_terminal_name.clone();
    }

    b.request_number = c.request_number;
    b.function_name = c.function_name.clone();

    let (_uac1, handle) = b.build();
    Ok(handle)
}

fn build_midi(c: &MidiConfig) -> Result<Handle> {
    let mut b = function::midi::Midi::builder();
    b.buflen = c.buflen;
    b.id = c.id.clone();
    b.in_ports = c.in_ports;
    b.out_ports = c.out_ports;
    b.index = c.index;
    b.qlen = c.qlen;
    let (_midi, handle) = b.build();
    Ok(handle)
}

fn build_msd(c: &MsdConfig) -> Result<Handle> {
    let mut b = function::msd::Msd::builder();
    b.stall = c.stall;
    for lun_cfg in &c.lun {
        let mut lun = if let Some(ref file) = lun_cfg.file {
            function::msd::Lun::new(PathBuf::from(file))?
        } else {
            function::msd::Lun::empty()
        };
        if let Some(v) = lun_cfg.read_only {
            lun.read_only = v;
        }
        if let Some(v) = lun_cfg.cdrom {
            lun.cdrom = v;
        }
        if let Some(v) = lun_cfg.no_fua {
            lun.no_fua = v;
        }
        if let Some(v) = lun_cfg.removable {
            lun.removable = v;
        }
        if let Some(ref v) = lun_cfg.inquiry_string {
            lun.inquiry_string = v.clone();
        }
        b.luns.push(lun);
    }
    let (_msd, handle) = b.build();
    Ok(handle)
}

fn build_printer(c: &PrinterConfig) -> Result<Handle> {
    let mut b = function::printer::Printer::builder();
    b.pnp_string = c.pnp_string.clone();
    b.qlen = c.qlen;
    let (_printer, handle) = b.build();
    Ok(handle)
}

fn build_loopback(c: &LoopbackConfig) -> Result<Handle> {
    let mut b = function::loopback::Loopback::builder();
    b.qlen = c.qlen;
    b.bulk_buflen = c.bulk_buflen;
    let (_loopback, handle) = b.build();
    Ok(handle)
}

fn build_source_sink(c: &SourceSinkConfig) -> Result<Handle> {
    let mut b = function::sourcesink::SourceSink::builder();
    b.pattern = c.pattern;
    b.isoc_interval = c.isoc_interval;
    b.isoc_maxpacket = c.isoc_maxpacket;
    b.isoc_mult = c.isoc_mult;
    b.isoc_maxburst = c.isoc_maxburst;
    b.bulk_buflen = c.bulk_buflen;
    b.bulk_qlen = c.bulk_qlen;
    b.iso_qlen = c.iso_qlen;
    let (_ss, handle) = b.build();
    Ok(handle)
}

fn build_video(c: &VideoConfig) -> Result<Handle> {
    let mut b = function::video::Uvc::builder();
    b.streaming_interval = c.streaming_interval;
    b.streaming_max_burst = c.streaming_max_burst;
    b.streaming_max_packet = c.streaming_max_packet;
    b.function_name = c.function_name.clone();
    b.processing_controls = c.processing_controls;
    b.camera_controls = c.camera_controls;

    if let Some(ref frames) = c.frame {
        for f in frames {
            let format = parse_video_format(&f.format)?;

            // Use the Frame helper to create UvcFrame (handles non_exhaustive).
            let fps = if let Some(ref fps_list) = f.fps {
                fps_list.clone()
            } else if f.intervals.is_some() {
                // Will set intervals directly below.
                vec![30]
            } else {
                vec![30]
            };

            let mut frame: function::video::UvcFrame =
                function::video::Frame::new(f.width, f.height, fps, format).into();

            // Override intervals if specified directly.
            if let Some(ref ivs) = f.intervals {
                frame.intervals = ivs.clone();
            }

            if let Some(ref cm) = f.color_matching {
                let mut color = function::video::ColorMatching::default();
                if let Some(v) = cm.color_primaries {
                    color.color_primaries = v;
                }
                if let Some(v) = cm.transfer_characteristics {
                    color.transfer_characteristics = v;
                }
                if let Some(v) = cm.matrix_coefficients {
                    color.matrix_coefficients = v;
                }
                frame.color_matching = Some(color);
            }

            b.frames.push(frame);
        }
    }

    let (_uvc, handle) = b.build();
    Ok(handle)
}

fn build_other(c: &OtherConfig) -> Result<Handle> {
    let mut b = function::other::Other::builder(&c.driver)?;
    if let Some(ref props) = c.properties {
        for (key, value) in props {
            b.set(key, value.as_bytes())?;
        }
    }
    let (_other, handle) = b.build();
    Ok(handle)
}

// --- Parsing helpers ---

fn parse_language(s: &str) -> Result<Language> {
    match s.to_lowercase().as_str() {
        "en-us" | "english" => Ok(Language::EnglishUnitedStates),
        "en-gb" => Ok(Language::EnglishUnitedKingdom),
        "de" | "german" => Ok(Language::GermanStandard),
        "fr" | "french" => Ok(Language::FrenchStandard),
        "es" | "spanish" => Ok(Language::SpanishTraditionalSort),
        "it" | "italian" => Ok(Language::ItalianStandard),
        "ja" | "japanese" => Ok(Language::Japanese),
        "ko" | "korean" => Ok(Language::Korean),
        "zh" | "chinese" => Ok(Language::ChinesePRC),
        "pt" | "portuguese" => Ok(Language::PortugueseBrazil),
        "ru" | "russian" => Ok(Language::Russian),
        "nl" | "dutch" => Ok(Language::DutchNetherlands),
        _ => {
            // Try as hex language code.
            if let Some(hex) = s.strip_prefix("0x") {
                let code = u16::from_str_radix(hex, 16)
                    .map_err(|e| Error::new(ErrorKind::InvalidInput, format!("bad language code: {e}")))?;
                Ok(Language::Other(code))
            } else {
                Err(Error::new(ErrorKind::InvalidInput, format!("unknown language: {s}")))
            }
        }
    }
}

fn parse_usb_version(s: &str) -> Result<UsbVersion> {
    match s {
        "1.1" => Ok(UsbVersion::V11),
        "2.0" => Ok(UsbVersion::V20),
        "3.0" => Ok(UsbVersion::V30),
        "3.1" => Ok(UsbVersion::V31),
        _ => {
            let v = parse_hex_u16(s)?;
            Ok(UsbVersion::Other(v))
        }
    }
}

fn parse_speed(s: &str) -> Result<Speed> {
    match s {
        "low-speed" | "low" => Ok(Speed::LowSpeed),
        "full-speed" | "full" => Ok(Speed::FullSpeed),
        "high-speed" | "high" => Ok(Speed::HighSpeed),
        "super-speed" | "super" => Ok(Speed::SuperSpeed),
        "super-speed-plus" | "super-plus" => Ok(Speed::SuperSpeedPlus),
        _ => Err(Error::new(ErrorKind::InvalidInput, format!("unknown speed: {s}"))),
    }
}

fn parse_video_format(s: &str) -> Result<function::video::Format> {
    match s.to_lowercase().as_str() {
        "yuyv" => Ok(function::video::Format::Yuyv),
        "mjpeg" => Ok(function::video::Format::Mjpeg),
        "nv12" => Ok(function::video::Format::nv12()),
        "h264" => Ok(function::video::Format::h264()),
        _ => Err(Error::new(ErrorKind::InvalidInput, format!("unknown video format: {s}"))),
    }
}

fn parse_hex_u16(s: &str) -> Result<u16> {
    if let Some(hex) = s.strip_prefix("0x") {
        u16::from_str_radix(hex, 16)
            .map_err(|e| Error::new(ErrorKind::InvalidInput, format!("bad hex value: {e}")))
    } else {
        s.parse::<u16>().map_err(|e| Error::new(ErrorKind::InvalidInput, format!("bad value: {e}")))
    }
}

fn parse_hex_bytes(s: &str) -> Result<Vec<u8>> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    if s.len() % 2 != 0 {
        return Err(Error::new(ErrorKind::InvalidInput, "hex string must have even length"));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16)
                .map_err(|e| Error::new(ErrorKind::InvalidInput, format!("bad hex byte: {e}")))
        })
        .collect()
}
