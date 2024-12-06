mod common;
use common::*;

use usb_gadget::function::printer::Printer;

#[test]
fn printer() {
    init();

    // Keyboard printer description
    let mut builder = Printer::builder();
    builder.pnp_string = Some("Rust Printer".to_string());
    builder.qlen = Some(20);
    let (printer, func) = builder.build();

    let reg = reg(func);

    println!("printer device at {}", printer.status().path().unwrap().display());

    unreg(reg).unwrap();
}
