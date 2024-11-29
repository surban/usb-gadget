mod common;
use common::*;

use usb_gadget::function::midi::Midi;

#[test]
fn midi() {
    init();

    let mut builder = Midi::builder();
    builder.id = Some("midi".to_string());
    builder.in_ports = Some(1);
    builder.out_ports = Some(1);
    let (midi, func) = builder.build();

    let reg = reg(func);

    println!("midi device at {}", midi.status().path().unwrap().display());

    unreg(reg).unwrap();
}
