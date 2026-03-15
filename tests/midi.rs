mod common;
use common::*;
use serial_test::serial;

use usb_gadget::function::midi::Midi;

const IN_PORTS: u8 = 2;
const OUT_PORTS: u8 = 3;

#[test]
#[serial]
fn midi() {
    init();

    let mut builder = Midi::builder();
    builder.id = Some("midi".to_string());
    builder.in_ports = Some(IN_PORTS);
    builder.out_ports = Some(OUT_PORTS);
    let (midi, func) = builder.build();

    let reg = reg(func);

    println!("midi device at {}", midi.status().path().unwrap().display());

    check_host(|_device, cfg| {
        // MIDI Streaming interface: Audio class 1, subclass 3.
        let intf = cfg.interface_alt_settings().find(|desc| desc.class() == 1 && desc.subclass() == 3);
        assert!(intf.is_some(), "no MIDI Streaming interface (class 1, subclass 3) found on host");
        let intf = intf.unwrap();
        println!(
            "MIDI Streaming interface {}: class={}, subclass={}, protocol={}",
            intf.interface_number(),
            intf.class(),
            intf.subclass(),
            intf.protocol(),
        );

        // Count bulk endpoints: one IN and one OUT expected for MIDI.
        let num_endpoints = intf.num_endpoints();
        assert!(num_endpoints >= 2, "expected at least 2 endpoints, found {num_endpoints}");
        println!("  endpoints: {num_endpoints}");

        // Count MIDI IN/OUT Jack descriptors from class-specific descriptors.
        // CS_INTERFACE (0x24): subtype 0x02 = MIDI IN Jack, subtype 0x03 = MIDI OUT Jack.
        let mut in_jacks = 0u8;
        let mut out_jacks = 0u8;
        for desc in intf.descriptors() {
            if desc.descriptor_type() == 0x24 && desc.len() >= 3 {
                match desc[2] {
                    0x02 => in_jacks += 1,
                    0x03 => out_jacks += 1,
                    _ => {}
                }
            }
        }
        println!("  MIDI IN Jacks: {in_jacks}, MIDI OUT Jacks: {out_jacks}");

        // Each port produces an Embedded + External jack pair in each direction.
        // in_ports: external IN jack + embedded IN jack per port (shows as IN jacks on host)
        // out_ports: external OUT jack + embedded OUT jack per port (shows as OUT jacks on host)
        // Total IN jacks = in_ports*2 + out_ports*2 (embedded+external for both directions)
        // Actually the kernel creates: per in_port 1 embedded_in + 1 external_out,
        // per out_port 1 external_in + 1 embedded_out.
        // So: IN jacks = in_ports (embedded) + out_ports (external), OUT jacks = out_ports (embedded) +
        // in_ports (external).
        let expected_in_jacks = IN_PORTS + OUT_PORTS;
        let expected_out_jacks = OUT_PORTS + IN_PORTS;
        assert_eq!(in_jacks, expected_in_jacks, "expected {expected_in_jacks} MIDI IN Jacks, found {in_jacks}");
        assert_eq!(
            out_jacks, expected_out_jacks,
            "expected {expected_out_jacks} MIDI OUT Jacks, found {out_jacks}"
        );
    });

    unreg(reg).unwrap();
}
