mod common;
use common::*;

use macaddr::MacAddr6;
use usb_gadget::function::net::{Net, NetClass};

fn net(net_class: NetClass) {
    init();
    let _mutex = exclusive();

    let dev_addr = MacAddr6::new(0x66, 0xf9, 0x7d, 0xf2, 0x3e, 0x2a);
    let host_addr = MacAddr6::new(0x7e, 0x21, 0xb2, 0xcb, 0xd4, 0x51);

    let mut builder = Net::builder(net_class);
    builder.dev_addr = Some(dev_addr);
    builder.host_addr = Some(host_addr);
    builder.qmult = Some(10);
    let (net, func) = builder.build();

    let reg = reg(func);

    println!(
        "Net device {} function at {}",
        net.ifname().unwrap().to_string_lossy(),
        net.path().unwrap().display()
    );

    assert_eq!(net.dev_addr().unwrap(), dev_addr);
    assert_eq!(net.host_addr().unwrap(), host_addr);

    if unreg(reg).unwrap() {
        assert!(net.path().is_err());
        assert!(net.dev_addr().is_err());
    }
}

#[test]
fn ecm() {
    net(NetClass::Ecm)
}

#[test]
fn ecm_subset() {
    net(NetClass::EcmSubset)
}

#[test]
fn eem() {
    net(NetClass::Eem)
}

#[test]
fn ncm() {
    net(NetClass::Ncm)
}

#[test]
fn rndis() {
    net(NetClass::Rndis)
}
