mod common;
use common::*;

use usb_gadget::function::midi::Midi;

// Ignored because it requires sound device index to be available which is not common on most systems
// on Raspberry Pi, the index is already in use by HDMI audio. Append 'noaudio' to 'dtoverlay=vc4-kms-v3d,noaudio' in /boot/config.txt: https://www.raspberrypi.com/documentation/computers/config_txt.html#hdmi-audio
#[ignore]
#[test]
fn midi() {
    init();

    let mut builder = Midi::builder();
    builder.index = 0;
    builder.id = "midi".to_string();
    builder.in_ports = 1;
    builder.out_ports = 1;
    let (midi, func) = builder.build();

    let reg = reg(func);

    println!("midi device at {}", midi.status().path().unwrap().display());

    unreg(reg).unwrap();
}
