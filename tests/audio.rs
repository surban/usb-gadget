mod common;
use common::*;

use usb_gadget::function::audio::{Channel, Uac2};

#[test]
fn audio() {
    init();

    let (audio, func) = Uac2::new(Channel::new(0b1111_1111, 48000, 24 / 8), Channel::new(0b11, 48000, 16 / 8));
    let reg = reg(func);

    println!("UAC2 audio device at {}", audio.status().path().unwrap().display());

    unreg(reg).unwrap();
}
