mod common;
use common::*;

#[test]
fn query_udcs() {
    init();

    let udcs = usb_gadget::udcs().unwrap();
    println!("USB device controllers:\n{:#?}", &udcs);

    for udc in udcs {
        println!("Name: {}", udc.name().to_string_lossy());
        println!("OTG: {:?}", udc.is_otg().unwrap());
        println!("Peripheral: {:?}", udc.is_a_peripheral().unwrap());
        println!("Current speed: {:?}", udc.current_speed().unwrap());
        println!("Max speed: {:?}", udc.max_speed().unwrap());
        println!("State: {:?}", udc.state().unwrap());
        println!("Function: {:?}", udc.function().unwrap());
        println!();
    }
}
