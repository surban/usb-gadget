mod common;
use common::*;
use serial_test::serial;

use usb_gadget::function::audio::{Channel, Uac2};

const CAPTURE_CHANNEL_MASK: u32 = 0b1111_1111;
const CAPTURE_SAMPLE_RATE: u32 = 48000;
const CAPTURE_SAMPLE_SIZE: u32 = 24 / 8;

const PLAYBACK_CHANNEL_MASK: u32 = 0b11;
const PLAYBACK_SAMPLE_RATE: u32 = 48000;
const PLAYBACK_SAMPLE_SIZE: u32 = 16 / 8;

#[test]
#[serial]
fn uac2() {
    init();

    let (audio, func) = Uac2::new(
        Channel::new(CAPTURE_CHANNEL_MASK, CAPTURE_SAMPLE_RATE, CAPTURE_SAMPLE_SIZE),
        Channel::new(PLAYBACK_CHANNEL_MASK, PLAYBACK_SAMPLE_RATE, PLAYBACK_SAMPLE_SIZE),
    );
    let reg = reg(func);

    println!("UAC2 audio device at {}", audio.status().path().unwrap().display());

    check_host(|_device, cfg| {
        // Verify AudioControl interface (class 1 = Audio, subclass 1 = AudioControl).
        let ac_intf = cfg.interface_alt_settings().find(|desc| desc.class() == 1 && desc.subclass() == 1);
        assert!(ac_intf.is_some(), "no AudioControl interface (class 1, subclass 1) found on host");
        println!("AudioControl interface {}", ac_intf.unwrap().interface_number(),);

        // Verify AudioStreaming interfaces (class 1, subclass 2).
        // UAC2 with both capture and playback should have at least 2 streaming alt settings.
        let as_intfs: Vec<_> =
            cfg.interface_alt_settings().filter(|desc| desc.class() == 1 && desc.subclass() == 2).collect();
        assert!(as_intfs.len() >= 2, "expected at least 2 AudioStreaming alt settings, found {}", as_intfs.len());

        // Collect Format Type I descriptors (CS_INTERFACE 0x24, subtype 0x02)
        // from all AudioStreaming alt settings to verify sample sizes.
        // UAC2 Format Type I: [bLength, 0x24, 0x02, bFormatType=0x01, bSubslotSize, bBitResolution]
        let mut sample_sizes: Vec<u8> = Vec::new();
        for as_intf in &as_intfs {
            for desc in as_intf.descriptors() {
                if desc.descriptor_type() == 0x24 && desc.len() >= 6 && desc[2] == 0x02 && desc[3] == 0x01 {
                    let sub_slot_size = desc[4];
                    let bit_resolution = desc[5];
                    println!(
                        "  AudioStreaming interface {}: Format Type I, subslot_size={}, bit_resolution={}",
                        as_intf.interface_number(),
                        sub_slot_size,
                        bit_resolution,
                    );
                    sample_sizes.push(sub_slot_size);
                }
            }
        }

        assert!(
            sample_sizes.contains(&(CAPTURE_SAMPLE_SIZE as u8)),
            "no AudioStreaming format with capture sample size {} found (got {:?})",
            CAPTURE_SAMPLE_SIZE,
            sample_sizes,
        );
        assert!(
            sample_sizes.contains(&(PLAYBACK_SAMPLE_SIZE as u8)),
            "no AudioStreaming format with playback sample size {} found (got {:?})",
            PLAYBACK_SAMPLE_SIZE,
            sample_sizes,
        );
    });

    unreg(reg).unwrap();
}
