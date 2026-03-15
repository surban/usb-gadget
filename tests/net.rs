mod common;
use common::*;
use serial_test::serial;

use macaddr::MacAddr6;
use nusb::MaybeFuture;
use std::{num::NonZeroU8, time::Duration};
use usb_gadget::function::net::{Net, NetClass};

fn net(net_class: NetClass) {
    init();

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
        net.status().path().unwrap().display()
    );

    assert_eq!(net.dev_addr().unwrap(), dev_addr);
    assert_eq!(net.host_addr().unwrap(), host_addr);

    check_host(|device, cfg| {
        let (expected_class, expected_subclass, label, has_mac_descriptor) = match net_class {
            NetClass::Ecm => (2, 6, "CDC ECM", true),
            NetClass::EcmSubset => (2, 10, "CDC ECM Subset", true),
            NetClass::Eem => (2, 12, "CDC EEM", false),
            NetClass::Ncm => (2, 13, "CDC NCM", true),
            NetClass::Rndis => (2, 2, "RNDIS", false),
            _ => return,
        };

        let intf = cfg
            .interface_alt_settings()
            .find(|desc| desc.class() == expected_class && desc.subclass() == expected_subclass);
        assert!(
            intf.is_some(),
            "no {label} interface (class {expected_class}, subclass {expected_subclass}) found on host"
        );
        let intf = intf.unwrap();
        println!(
            "{label} interface {}: class={}, subclass={}, protocol={}",
            intf.interface_number(),
            intf.class(),
            intf.subclass(),
            intf.protocol(),
        );

        // Verify MAC address from CDC Ethernet Networking Functional Descriptor.
        // This descriptor (CS_INTERFACE type 0x24, subtype 0x0F) contains an
        // iMACAddress string descriptor index at byte 3. Available for ECM,
        // ECM Subset, and NCM; not present in EEM and RNDIS.
        if has_mac_descriptor {
            let eth_desc =
                intf.descriptors().find(|d| d.descriptor_type() == 0x24 && d.len() >= 4 && d[2] == 0x0F);
            assert!(eth_desc.is_some(), "no CDC Ethernet Networking Functional Descriptor found");
            let eth_desc = eth_desc.unwrap();
            let mac_string_idx =
                NonZeroU8::new(eth_desc[3]).expect("iMACAddress string descriptor index is zero");

            let mac_string = device
                .get_string_descriptor(mac_string_idx, 0x0409, Duration::from_secs(1))
                .wait()
                .expect("failed to read iMACAddress string descriptor");

            let expected_mac = format!(
                "{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
                host_addr.as_bytes()[0],
                host_addr.as_bytes()[1],
                host_addr.as_bytes()[2],
                host_addr.as_bytes()[3],
                host_addr.as_bytes()[4],
                host_addr.as_bytes()[5],
            );
            assert_eq!(
                mac_string, expected_mac,
                "host MAC address mismatch: got {mac_string}, expected {expected_mac}"
            );
            println!("{label} iMACAddress: {mac_string}");
        }
    });

    if unreg(reg).unwrap() {
        assert!(net.status().path().is_none());
        assert!(net.dev_addr().is_err());
    }
}

#[test]
#[serial]
fn ecm() {
    net(NetClass::Ecm)
}

#[test]
#[serial]
fn ecm_subset() {
    net(NetClass::EcmSubset)
}

#[test]
#[serial]
fn eem() {
    net(NetClass::Eem)
}

#[test]
#[serial]
fn ncm() {
    net(NetClass::Ncm)
}

#[test]
#[serial]
fn rndis() {
    net(NetClass::Rndis)
}
