mod common;
use common::*;

use usb_gadget::function::audio::{Channel, Uac1};

const CAPTURE_CHANNEL_MASK: u32 = 0b11;
const CAPTURE_SAMPLE_RATE: u32 = 48000;
const CAPTURE_SAMPLE_SIZE: u32 = 2;

const PLAYBACK_CHANNEL_MASK: u32 = 0b11;
const PLAYBACK_SAMPLE_RATE: u32 = 48000;
const PLAYBACK_SAMPLE_SIZE: u32 = 2;

#[test]
fn uac1() {
    init();
    let _mutex = exclusive();

    let (audio, func) = Uac1::new(
        Channel::new(CAPTURE_CHANNEL_MASK, CAPTURE_SAMPLE_RATE, CAPTURE_SAMPLE_SIZE),
        Channel::new(PLAYBACK_CHANNEL_MASK, PLAYBACK_SAMPLE_RATE, PLAYBACK_SAMPLE_SIZE),
    );
    let reg = reg(func);

    println!("UAC1 audio device at {}", audio.status().path().unwrap().display());

    check_host(|_device, cfg| {
        // Verify AudioControl interface (class 1 = Audio, subclass 1 = AudioControl).
        let ac_intf = cfg.interface_alt_settings().find(|desc| desc.class() == 1 && desc.subclass() == 1);
        assert!(ac_intf.is_some(), "no AudioControl interface (class 1, subclass 1) found on host");
        println!("AudioControl interface {}", ac_intf.unwrap().interface_number());

        // Verify AudioStreaming interfaces (class 1, subclass 2).
        // UAC1 with both capture and playback should have at least 2 streaming alt settings.
        let as_intfs: Vec<_> =
            cfg.interface_alt_settings().filter(|desc| desc.class() == 1 && desc.subclass() == 2).collect();
        assert!(as_intfs.len() >= 2, "expected at least 2 AudioStreaming alt settings, found {}", as_intfs.len());

        // Collect Format Type I descriptors (CS_INTERFACE 0x24, subtype 0x02)
        // from all AudioStreaming alt settings to verify sample sizes.
        // UAC1 Format Type I: [bLength, 0x24, 0x02, bFormatType=0x01, bNrChannels, ...]
        let mut found_streaming = false;
        for as_intf in &as_intfs {
            for desc in as_intf.descriptors() {
                if desc.descriptor_type() == 0x24 && desc.len() >= 6 && desc[2] == 0x02 && desc[3] == 0x01 {
                    let nr_channels = desc[4];
                    let sub_frame_size = desc[5];
                    println!(
                        "  AudioStreaming interface {}: Format Type I, channels={}, sub_frame_size={}",
                        as_intf.interface_number(),
                        nr_channels,
                        sub_frame_size,
                    );
                    found_streaming = true;
                }
            }
        }
        assert!(found_streaming, "no Format Type I descriptors found in AudioStreaming interfaces");
    });

    unreg(reg).unwrap();
}

#[test]
fn uac1_builder() {
    init();
    let _mutex = exclusive();

    let mut builder = Uac1::builder();
    builder.capture.channel = Channel::new(CAPTURE_CHANNEL_MASK, CAPTURE_SAMPLE_RATE, CAPTURE_SAMPLE_SIZE);
    builder.playback.channel = Channel::new(PLAYBACK_CHANNEL_MASK, PLAYBACK_SAMPLE_RATE, PLAYBACK_SAMPLE_SIZE);
    builder.capture.mute_present = Some(true);
    builder.capture.volume_present = Some(true);
    builder.playback.mute_present = Some(true);
    builder.playback.volume_present = Some(true);
    let (audio, func) = builder.build();

    let reg = reg(func);

    println!("UAC1 audio (builder) device at {}", audio.status().path().unwrap().display());

    check_host(|_device, cfg| {
        let ac_intf = cfg.interface_alt_settings().find(|desc| desc.class() == 1 && desc.subclass() == 1);
        assert!(ac_intf.is_some(), "no AudioControl interface found on host");
    });

    unreg(reg).unwrap();
}
