mod common;
use common::*;

use usb_gadget::function::midi::Midi;

#[test]
fn midi() {
    init();

    let mut builder = Midi::builder();
    builder.buflen = 64;
    builder.id = "midi".to_string();
    builder.in_ports = 1;
    builder.out_ports = 1;
    builder.index = 0;
    builder.qlen = 8;
    let (midi, func) = builder.build();

    let reg = reg(func);

    println!("midi device {:?} at {}", midi.device().unwrap(), midi.status().path().unwrap().display());

    unreg(reg).unwrap();
}
