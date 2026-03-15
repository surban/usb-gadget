use std::{thread, time::Duration};
use uuid::uuid;

use nusb::MaybeFuture;
use serial_test::serial;
use usb_gadget::{
    default_udc,
    function::custom::{Custom, Endpoint, EndpointDirection, Interface, OsExtCompat, OsExtProp},
    Class,
};

#[path = "../common/mod.rs"]
mod common;
use common::*;

mod transfer;
mod zlp;

#[test]
#[serial]
fn custom() {
    init();

    let (mut ep1_rx, ep1_dir) = EndpointDirection::host_to_device();
    let (mut ep2_tx, ep2_dir) = EndpointDirection::device_to_host();

    let (custom, handle) = Custom::builder()
        .with_interface(
            Interface::new(Class::vendor_specific(1, 1), "custom interface")
                .with_endpoint(Endpoint::bulk(ep1_dir))
                .with_endpoint(Endpoint::bulk(ep2_dir)),
        )
        .build();

    let reg = reg(handle);
    println!("Custom function at {}", custom.status().unwrap().path().unwrap().display());
    println!();

    println!("Getting ep1_rx control");
    let _ep1_control = ep1_rx.control().unwrap();

    println!("Getting ep2_tx control");
    let _ep2_control = ep2_tx.control().unwrap();

    check_host(|_device, cfg| {
        let intf = cfg
            .interface_alt_settings()
            .find(|desc| desc.class() == 0xFF && desc.subclass() == 1 && desc.protocol() == 1);
        assert!(
            intf.is_some(),
            "no vendor-specific interface (class 0xFF, subclass 1, protocol 1) found on host"
        );
        let intf = intf.unwrap();
        assert_eq!(intf.num_endpoints(), 2, "expected 2 bulk endpoints");
        println!(
            "Custom interface {}: class=0x{:02X}, subclass={}, protocol={}, endpoints={}",
            intf.interface_number(),
            intf.class(),
            intf.subclass(),
            intf.protocol(),
            intf.num_endpoints(),
        );
    });

    println!("Unregistering");
    if unreg(reg).unwrap() {
        assert!(custom.status().unwrap().path().is_none());
    }
}

#[test]
#[serial]
fn custom_with_os_desc() {
    init();

    let (mut ep1_rx, ep1_dir) = EndpointDirection::host_to_device();
    let (mut ep2_tx, ep2_dir) = EndpointDirection::device_to_host();

    let (mut custom, handle) = Custom::builder()
        .with_interface(
            Interface::new(Class::vendor_specific(1, 1), "custom interface")
                .with_endpoint(Endpoint::bulk(ep1_dir))
                .with_endpoint(Endpoint::bulk(ep2_dir))
                .with_os_ext_compat(OsExtCompat::winusb())
                .with_os_ext_prop(OsExtProp::device_interface_guid(uuid!(
                    "8FE6D4D7-49DD-41E7-9486-49AFC6BFE475"
                ))),
        )
        .build();

    let reg = reg_with_os_desc(handle);
    println!("Custom function at {}", custom.status().unwrap().path().unwrap().display());
    println!("real interface address 0: {}", custom.real_address(0).unwrap());
    println!();

    let ep1_control = ep1_rx.control().unwrap();
    println!("ep1 unclaimed: {:?}", ep1_control.unclaimed_fifo());
    println!("ep1 real address: {}", ep1_control.real_address().unwrap());
    println!("ep1 descriptor: {:?}", ep1_control.descriptor().unwrap());
    println!();

    let ep2_control = ep2_tx.control().unwrap();
    println!("ep2 unclaimed: {:?}", ep2_control.unclaimed_fifo());
    println!("ep2 real address: {}", ep2_control.real_address().unwrap());
    println!("ep2 descriptor: {:?}", ep2_control.descriptor().unwrap());
    println!();

    if !skip_host() {
        // reg_with_os_desc uses VID=6, PID=0x11.
        let dev_info = find_device_with_id(6, 0x11);
        println!("found device on host: {dev_info:?}");
        let device = dev_info.open().wait().expect("cannot open device on host");
        let cfg = device.active_configuration().expect("no active configuration");

        // Verify vendor-specific interface with 2 bulk endpoints.
        let intf = cfg
            .interface_alt_settings()
            .find(|desc| desc.class() == 0xFF && desc.subclass() == 1 && desc.protocol() == 1);
        assert!(
            intf.is_some(),
            "no vendor-specific interface (class 0xFF, subclass 1, protocol 1) found on host"
        );
        let intf = intf.unwrap();
        assert_eq!(intf.num_endpoints(), 2, "expected 2 bulk endpoints");
        println!(
            "Custom interface {}: class=0x{:02X}, subclass={}, protocol={}, endpoints={}",
            intf.interface_number(),
            intf.class(),
            intf.subclass(),
            intf.protocol(),
            intf.num_endpoints(),
        );

        // Read BOS descriptor (type 0x0F) to verify platform capabilities.
        let bos = device.get_descriptor(0x0F, 0, 0, Duration::from_secs(1)).wait();
        match bos {
            Ok(bos_data) => {
                println!("BOS descriptor: {} bytes", bos_data.len());

                // Parse BOS: [bLength, bDescriptorType=0x0F, wTotalLength(2), bNumDeviceCaps]
                assert!(bos_data.len() >= 5, "BOS descriptor too short");
                let num_caps = bos_data[4];
                println!("  bNumDeviceCaps: {num_caps}");

                // Scan for Platform Capability descriptors (bDevCapabilityType = 0x05).
                // Each has a 128-bit PlatformCapabilityUUID at offset 4.
                // WebUSB: 3408B638-09A9-47A0-8BFD-A0768815B665
                let webusb_uuid: [u8; 16] = [
                    0x38, 0xB6, 0x08, 0x34, 0xA9, 0x09, 0xA0, 0x47, 0x8B, 0xFD, 0xA0, 0x76, 0x88, 0x15, 0xB6,
                    0x65,
                ];

                let mut found_webusb = false;
                let mut offset = 5; // skip BOS header
                while offset + 4 < bos_data.len() {
                    let cap_len = bos_data[offset] as usize;
                    if cap_len < 4 || offset + cap_len > bos_data.len() {
                        break;
                    }
                    let cap_type = bos_data[offset + 2]; // bDevCapabilityType
                    if cap_type == 0x05 && cap_len >= 20 {
                        // Platform capability: UUID at offset+4..offset+20.
                        let uuid = &bos_data[offset + 4..offset + 20];
                        if uuid == webusb_uuid {
                            found_webusb = true;
                            println!("  Found WebUSB Platform Capability");
                        }
                    }
                    offset += cap_len;
                }

                assert!(found_webusb, "WebUSB Platform Capability not found in BOS descriptor");
            }
            Err(e) => {
                println!("Could not read BOS descriptor: {e} (kernel may not support it)");
            }
        }

        // Verify MS OS 1.0 string descriptor at index 0xEE.
        // The raw descriptor contains the UTF-16LE encoded "MSFT100" signature
        // followed by the vendor code byte (0xf0) and a padding byte.
        let ms_os_str = device.get_descriptor(0x03, 0xEE, 0, Duration::from_secs(1)).wait();
        match ms_os_str {
            Ok(data) => {
                println!("MS OS 1.0 string descriptor (0xEE): {} bytes: {:02X?}", data.len(), &data[..]);
                // Decode UTF-16LE signature from bytes 2..16 (7 chars = 14 bytes).
                assert!(data.len() >= 18, "MS OS 1.0 descriptor too short");
                let sig: String = data[2..16].chunks_exact(2).map(|c| char::from(c[0])).collect();
                assert_eq!(sig, "MSFT100", "MS OS 1.0 signature mismatch: got {sig:?}");
                let vendor_code = data[16];
                assert_eq!(vendor_code, 0xf0, "MS OS 1.0 vendor code mismatch: got 0x{vendor_code:02x}");
                println!("  MS OS 1.0: signature={sig:?}, vendor_code=0x{vendor_code:02x}");
            }
            Err(e) => {
                println!("Could not read MS OS 1.0 string descriptor: {e}");
            }
        }
    }

    println!("Unregistering");
    if unreg(reg).unwrap() {
        assert!(custom.status().unwrap().path().is_none());
    }
}

#[test]
#[serial]
fn custom_no_disconnect() {
    init();

    let (reg, ffs_dir) = {
        let (mut ep1_rx, ep1_dir) = EndpointDirection::host_to_device();
        let (mut ep2_tx, ep2_dir) = EndpointDirection::device_to_host();

        let mut builder = Custom::builder().with_interface(
            Interface::new(Class::vendor_specific(1, 1), "custom interface")
                .with_endpoint(Endpoint::bulk(ep1_dir))
                .with_endpoint(Endpoint::bulk(ep2_dir)),
        );
        builder.ffs_no_disconnect = true;
        builder.ffs_file_mode = Some(0o770);
        builder.ffs_root_mode = Some(0o777);

        let (mut custom, handle) = builder.build();

        let reg = reg(handle);
        println!("Custom function at {}", custom.status().unwrap().path().unwrap().display());
        println!();

        let ffs_dir = custom.ffs_dir().unwrap();
        println!("FunctionFS is at {}", ffs_dir.display());

        println!("Getting ep1_rx control");
        let _ep1_control = ep1_rx.control().unwrap();

        println!("Getting ep2_tx control");
        let _ep2_control = ep2_tx.control().unwrap();

        thread::sleep(Duration::from_secs(3));

        println!("Dropping custom interface");
        (reg, ffs_dir)
    };

    println!("Dropped custom interface");
    thread::sleep(Duration::from_secs(3));

    {
        println!("Recreating custom interface using existing FunctionFS mount");

        let (mut ep1_rx, ep1_dir) = EndpointDirection::host_to_device();
        let (mut ep2_tx, ep2_dir) = EndpointDirection::device_to_host();
        let mut custom = Custom::builder()
            .with_interface(
                Interface::new(Class::vendor_specific(1, 1), "custom interface")
                    .with_endpoint(Endpoint::bulk(ep1_dir))
                    .with_endpoint(Endpoint::bulk(ep2_dir)),
            )
            .existing(&ffs_dir)
            .unwrap();

        assert_eq!(ffs_dir, custom.ffs_dir().unwrap());

        println!("Getting ep1_rx control");
        let _ep1_control = ep1_rx.control().unwrap();

        println!("Getting ep2_tx control");
        let _ep2_control = ep2_tx.control().unwrap();

        println!("Reactivating USB gadget");
        reg.bind(Some(&default_udc().unwrap())).unwrap();

        check_host(|_device, cfg| {
            let intf = cfg
                .interface_alt_settings()
                .find(|desc| desc.class() == 0xFF && desc.subclass() == 1 && desc.protocol() == 1);
            assert!(
                intf.is_some(),
                "no vendor-specific interface (class 0xFF, subclass 1, protocol 1) found on host"
            );
            let intf = intf.unwrap();
            assert_eq!(intf.num_endpoints(), 2, "expected 2 bulk endpoints");
            println!(
                "Custom interface {}: class=0x{:02X}, subclass={}, protocol={}, endpoints={}",
                intf.interface_number(),
                intf.class(),
                intf.subclass(),
                intf.protocol(),
                intf.num_endpoints(),
            );
        });

        println!("Dropping custom interface");
    }

    println!("Unregistering");
    unreg(reg).unwrap();
}

#[test]
#[serial]
fn custom_ext_init() {
    init();

    let (reg, ffs_dir) = {
        let mut builder = Custom::builder();
        builder.ffs_no_init = true;

        let (mut custom, handle) = builder.build();

        let reg = reg_no_bind(handle);
        println!("Custom function at {}", custom.status().unwrap().path().unwrap().display());
        println!();

        let ffs_dir = custom.ffs_dir().unwrap();
        println!("FunctionFS is at {}", ffs_dir.display());
        custom.fd().expect_err("fd must not be available");

        thread::sleep(Duration::from_secs(3));

        println!("Dropping custom interface");
        (reg, ffs_dir)
    };

    println!("Dropped custom interface");
    thread::sleep(Duration::from_secs(3));

    {
        println!("Creating custom interface using existing FunctionFS mount");

        let (mut ep1_rx, ep1_dir) = EndpointDirection::host_to_device();
        let (mut ep2_tx, ep2_dir) = EndpointDirection::device_to_host();
        let mut custom = Custom::builder()
            .with_interface(
                Interface::new(Class::vendor_specific(1, 1), "custom interface")
                    .with_endpoint(Endpoint::bulk(ep1_dir))
                    .with_endpoint(Endpoint::bulk(ep2_dir)),
            )
            .existing(&ffs_dir)
            .unwrap();

        assert_eq!(ffs_dir, custom.ffs_dir().unwrap());
        assert!(custom.status().is_none());

        println!("Getting ep1_rx control");
        let _ep1_control = ep1_rx.control().unwrap();

        println!("Getting ep2_tx control");
        let _ep2_control = ep2_tx.control().unwrap();

        println!("Activating USB gadget");
        reg.bind(Some(&default_udc().unwrap())).unwrap();

        check_host(|_device, cfg| {
            let intf = cfg
                .interface_alt_settings()
                .find(|desc| desc.class() == 0xFF && desc.subclass() == 1 && desc.protocol() == 1);
            assert!(
                intf.is_some(),
                "no vendor-specific interface (class 0xFF, subclass 1, protocol 1) found on host"
            );
            let intf = intf.unwrap();
            assert_eq!(intf.num_endpoints(), 2, "expected 2 bulk endpoints");
            println!(
                "Custom interface {}: class=0x{:02X}, subclass={}, protocol={}, endpoints={}",
                intf.interface_number(),
                intf.class(),
                intf.subclass(),
                intf.protocol(),
                intf.num_endpoints(),
            );
        });

        println!("Dropping custom interface");
    }

    println!("Unregistering");
    unreg(reg).unwrap();
}
