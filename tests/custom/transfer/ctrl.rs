//! Control request tests: recv, recv_all, halt, and drop-without-reply paths.
//!
//! Tests for [`CtrlReceiver`] and [`CtrlSender`], covering the success paths
//! as well as explicit `.halt()` and implicit stall via `Drop`.

use nusb::{
    transfer::{ControlIn, ControlOut, ControlType, Recipient, TransferError},
    MaybeFuture,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use usb_gadget::function::custom::{Custom, Event};

use super::*;

const VID: u16 = 0x1234;
const PID: u16 = 0x0030;

/// Vendor request codes for these tests.
mod ctrl_req {
    /// Echo: host sends data, device stores it, then host reads it back.
    pub const ECHO: u8 = 1;
    /// Host sends data, device calls `recv()` (not `recv_all`) with an exact-size buffer.
    pub const RECV_BUF: u8 = 2;
    /// Host sends data, device explicitly calls `halt()` on the CtrlReceiver (rejects it).
    pub const HALT_RECV: u8 = 3;
    /// Host requests data, device explicitly calls `halt()` on the CtrlSender (rejects it).
    pub const HALT_SEND: u8 = 4;
    /// Host sends data, device drops the CtrlReceiver without calling recv or halt.
    pub const DROP_RECV: u8 = 5;
    /// Host requests data, device drops the CtrlSender without calling send or halt.
    pub const DROP_SEND: u8 = 6;
    /// Stop the device event loop.
    pub const STOP: u8 = 255;
}

// ─── Device side ────────────────────────────────────────────────────

fn run_device_ctrl_events(custom: &mut Custom, stop: &AtomicBool) {
    let mut ctrl_data = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        match custom.event_timeout(Duration::from_secs(1)) {
            Ok(Some(event)) => match event {
                Event::SetupHostToDevice(req) => {
                    let code = req.ctrl_req().request;
                    match code {
                        ctrl_req::STOP => {
                            stop.store(true, Ordering::Relaxed);
                            req.recv_all().unwrap();
                        }
                        ctrl_req::ECHO => {
                            ctrl_data = req.recv_all().unwrap();
                        }
                        ctrl_req::RECV_BUF => {
                            // Use recv() with an exact-size buffer instead of recv_all().
                            let len = req.len();
                            let mut buf = vec![0u8; len];
                            let n = req.recv(&mut buf).unwrap();
                            buf.truncate(n);
                            ctrl_data = buf;
                        }
                        ctrl_req::HALT_RECV => {
                            // Explicit halt: reject the host-to-device data.
                            req.halt().unwrap();
                            println!("device: halted CtrlReceiver (request {code})");
                        }
                        ctrl_req::DROP_RECV => {
                            // Drop without calling recv or halt → implicit stall via Drop.
                            println!("device: dropping CtrlReceiver (request {})", code);
                            drop(req);
                        }
                        _ => {
                            // Consume unknown requests to avoid stalling unexpectedly.
                            let _ = req.recv_all();
                        }
                    }
                }
                Event::SetupDeviceToHost(req) => {
                    let code = req.ctrl_req().request;
                    match code {
                        ctrl_req::ECHO => {
                            req.send(&ctrl_data).unwrap();
                        }
                        ctrl_req::HALT_SEND => {
                            // Explicit halt: reject the device-to-host request.
                            req.halt().unwrap();
                            println!("device: halted CtrlSender (request {code})");
                        }
                        ctrl_req::DROP_SEND => {
                            // Drop without calling send or halt → implicit stall via Drop.
                            println!("device: dropping CtrlSender (request {})", code);
                            drop(req);
                        }
                        _ => {
                            let _ = req.send(&[]);
                        }
                    }
                }
                _ => {}
            },
            Ok(None) => {}
            Err(e) => {
                if !stop.load(Ordering::Relaxed) {
                    panic!("device event error: {e}");
                }
                break;
            }
        }
    }
}

// ─── Host side helpers ──────────────────────────────────────────────

fn host_control_out(intf: &nusb::Interface, if_num: u8, request: u8, data: &[u8]) -> Result<(), TransferError> {
    intf.control_out(
        ControlOut {
            control_type: ControlType::Vendor,
            recipient: Recipient::Interface,
            request,
            value: 0,
            index: if_num.into(),
            data,
        },
        Duration::from_secs(2),
    )
    .wait()
}

fn host_control_in(
    intf: &nusb::Interface, if_num: u8, request: u8, length: u16,
) -> Result<Vec<u8>, TransferError> {
    intf.control_in(
        ControlIn {
            control_type: ControlType::Vendor,
            recipient: Recipient::Interface,
            request,
            value: 0,
            index: if_num.into(),
            length,
        },
        Duration::from_secs(2),
    )
    .wait()
}

fn host_stop(intf: &nusb::Interface, if_num: u8) {
    host_control_out(intf, if_num, ctrl_req::STOP, &[]).expect("host: stop control failed");
}

// ─── Tests ──────────────────────────────────────────────────────────

/// Test CtrlReceiver::recv_all (success path) and CtrlSender::send (success path)
/// via an echo round-trip.
#[test]
fn ctrl_recv_all_and_send() {
    init();
    let _mutex = exclusive();

    if skip_host() {
        return;
    }

    let (reg, mut custom, _ep_rx, _ep_tx) =
        setup_gadget_with_id(VID, PID, "ctrl recv_all test", "ctrl-recv-all-001");

    let stop = Arc::new(AtomicBool::new(false));
    let stop_dev = stop.clone();

    thread::scope(|s| {
        s.spawn(move || run_device_ctrl_events(&mut custom, &stop_dev));

        s.spawn(|| {
            let (_intf, _ep_in, _ep_out, if_num) = open_host_device(VID, PID);

            let test_data: Vec<u8> = (0..64).collect();

            // Send data to device (CtrlReceiver::recv_all on device side).
            host_control_out(&_intf, if_num, ctrl_req::ECHO, &test_data).expect("host: echo out failed");

            // Read it back (CtrlSender::send on device side).
            let reply = host_control_in(&_intf, if_num, ctrl_req::ECHO, test_data.len() as u16)
                .expect("host: echo in failed");
            assert_eq!(reply.as_slice(), test_data.as_slice(), "echo mismatch");
            println!("host: ctrl_recv_all_and_send echo OK ({} bytes)", test_data.len());

            host_stop(&_intf, if_num);
        });
    });

    thread::sleep(Duration::from_millis(500));
    reg.remove().unwrap();
}

/// Test CtrlReceiver::recv (with explicit buffer) via echo round-trip.
#[test]
fn ctrl_recv_buf() {
    init();
    let _mutex = exclusive();

    if skip_host() {
        return;
    }

    let (reg, mut custom, _ep_rx, _ep_tx) =
        setup_gadget_with_id(VID, PID, "ctrl recv_buf test", "ctrl-recv-buf-001");

    let stop = Arc::new(AtomicBool::new(false));
    let stop_dev = stop.clone();

    thread::scope(|s| {
        s.spawn(move || run_device_ctrl_events(&mut custom, &stop_dev));

        s.spawn(|| {
            let (_intf, _ep_in, _ep_out, if_num) = open_host_device(VID, PID);

            let test_data: Vec<u8> = (0..32).collect();

            // Send data to device using RECV_BUF (CtrlReceiver::recv on device side).
            host_control_out(&_intf, if_num, ctrl_req::RECV_BUF, &test_data).expect("host: recv_buf out failed");

            // Read it back via ECHO.
            let reply = host_control_in(&_intf, if_num, ctrl_req::ECHO, test_data.len() as u16)
                .expect("host: echo in failed");
            assert_eq!(reply.as_slice(), test_data.as_slice(), "recv_buf echo mismatch");
            println!("host: ctrl_recv_buf echo OK ({} bytes)", test_data.len());

            host_stop(&_intf, if_num);
        });
    });

    thread::sleep(Duration::from_millis(500));
    reg.remove().unwrap();
}

/// Test CtrlReceiver::halt (explicit stall on host-to-device request).
#[test]
fn ctrl_recv_halt() {
    init();
    let _mutex = exclusive();

    if skip_host() {
        return;
    }

    let (reg, mut custom, _ep_rx, _ep_tx) =
        setup_gadget_with_id(VID, PID, "ctrl recv halt test", "ctrl-recv-halt-001");

    let stop = Arc::new(AtomicBool::new(false));
    let stop_dev = stop.clone();

    thread::scope(|s| {
        s.spawn(move || run_device_ctrl_events(&mut custom, &stop_dev));

        s.spawn(|| {
            let (_intf, _ep_in, _ep_out, if_num) = open_host_device(VID, PID);

            let test_data = vec![0xAA; 16];

            // Device will call halt() on the CtrlReceiver → host should see STALL.
            let result = host_control_out(&_intf, if_num, ctrl_req::HALT_RECV, &test_data);
            match result {
                Err(TransferError::Stall) => {
                    println!("host: ctrl_recv_halt got expected STALL");
                }
                Err(e) => panic!("host: ctrl_recv_halt expected Stall, got: {e}"),
                Ok(()) => panic!("host: ctrl_recv_halt expected Stall, but transfer succeeded"),
            }

            // Verify the device still works: a normal echo should succeed.
            let echo_data: Vec<u8> = (0..8).collect();
            host_control_out(&_intf, if_num, ctrl_req::ECHO, &echo_data)
                .expect("host: post-halt echo out failed");
            let reply = host_control_in(&_intf, if_num, ctrl_req::ECHO, echo_data.len() as u16)
                .expect("host: post-halt echo in failed");
            assert_eq!(reply.as_slice(), echo_data.as_slice(), "post-halt echo mismatch");
            println!("host: post-halt echo OK");

            host_stop(&_intf, if_num);
        });
    });

    thread::sleep(Duration::from_millis(500));
    reg.remove().unwrap();
}

/// Test CtrlSender::halt (explicit stall on device-to-host request).
#[test]
fn ctrl_send_halt() {
    init();
    let _mutex = exclusive();

    if skip_host() {
        return;
    }

    let (reg, mut custom, _ep_rx, _ep_tx) =
        setup_gadget_with_id(VID, PID, "ctrl send halt test", "ctrl-send-halt-001");

    let stop = Arc::new(AtomicBool::new(false));
    let stop_dev = stop.clone();

    thread::scope(|s| {
        s.spawn(move || run_device_ctrl_events(&mut custom, &stop_dev));

        s.spawn(|| {
            let (_intf, _ep_in, _ep_out, if_num) = open_host_device(VID, PID);

            // Device will call halt() on the CtrlSender → host should see STALL.
            let result = host_control_in(&_intf, if_num, ctrl_req::HALT_SEND, 64);
            match result {
                Err(TransferError::Stall) => {
                    println!("host: ctrl_send_halt got expected STALL");
                }
                Err(e) => panic!("host: ctrl_send_halt expected Stall, got: {e}"),
                Ok(data) => panic!("host: ctrl_send_halt expected Stall, but got {} bytes", data.len()),
            }

            // Verify the device still works after the stall.
            let echo_data: Vec<u8> = (0..8).collect();
            host_control_out(&_intf, if_num, ctrl_req::ECHO, &echo_data)
                .expect("host: post-halt echo out failed");
            let reply = host_control_in(&_intf, if_num, ctrl_req::ECHO, echo_data.len() as u16)
                .expect("host: post-halt echo in failed");
            assert_eq!(reply.as_slice(), echo_data.as_slice(), "post-halt echo mismatch");
            println!("host: post-halt echo OK");

            host_stop(&_intf, if_num);
        });
    });

    thread::sleep(Duration::from_millis(500));
    reg.remove().unwrap();
}

/// Test CtrlReceiver drop without recv or halt (implicit stall via Drop).
#[test]
fn ctrl_recv_drop_stall() {
    init();
    let _mutex = exclusive();

    if skip_host() {
        return;
    }

    let (reg, mut custom, _ep_rx, _ep_tx) =
        setup_gadget_with_id(VID, PID, "ctrl recv drop test", "ctrl-recv-drop-001");

    let stop = Arc::new(AtomicBool::new(false));
    let stop_dev = stop.clone();

    thread::scope(|s| {
        s.spawn(move || run_device_ctrl_events(&mut custom, &stop_dev));

        s.spawn(|| {
            let (_intf, _ep_in, _ep_out, if_num) = open_host_device(VID, PID);

            let test_data = vec![0xBB; 16];

            // Device will drop the CtrlReceiver without calling recv or halt → implicit STALL.
            let result = host_control_out(&_intf, if_num, ctrl_req::DROP_RECV, &test_data);
            match result {
                Err(TransferError::Stall) => {
                    println!("host: ctrl_recv_drop_stall got expected STALL");
                }
                Err(e) => panic!("host: ctrl_recv_drop_stall expected Stall, got: {e}"),
                Ok(()) => panic!("host: ctrl_recv_drop_stall expected Stall, but transfer succeeded"),
            }

            // Verify the device still works after the implicit stall.
            let echo_data: Vec<u8> = (0..8).collect();
            host_control_out(&_intf, if_num, ctrl_req::ECHO, &echo_data)
                .expect("host: post-drop echo out failed");
            let reply = host_control_in(&_intf, if_num, ctrl_req::ECHO, echo_data.len() as u16)
                .expect("host: post-drop echo in failed");
            assert_eq!(reply.as_slice(), echo_data.as_slice(), "post-drop echo mismatch");
            println!("host: post-drop echo OK");

            host_stop(&_intf, if_num);
        });
    });

    thread::sleep(Duration::from_millis(500));
    reg.remove().unwrap();
}

/// Test CtrlSender drop without send or halt (implicit stall via Drop).
#[test]
fn ctrl_send_drop_stall() {
    init();
    let _mutex = exclusive();

    if skip_host() {
        return;
    }

    let (reg, mut custom, _ep_rx, _ep_tx) =
        setup_gadget_with_id(VID, PID, "ctrl send drop test", "ctrl-send-drop-001");

    let stop = Arc::new(AtomicBool::new(false));
    let stop_dev = stop.clone();

    thread::scope(|s| {
        s.spawn(move || run_device_ctrl_events(&mut custom, &stop_dev));

        s.spawn(|| {
            let (_intf, _ep_in, _ep_out, if_num) = open_host_device(VID, PID);

            // Device will drop the CtrlSender without calling send or halt → implicit STALL.
            let result = host_control_in(&_intf, if_num, ctrl_req::DROP_SEND, 64);
            match result {
                Err(TransferError::Stall) => {
                    println!("host: ctrl_send_drop_stall got expected STALL");
                }
                Err(e) => panic!("host: ctrl_send_drop_stall expected Stall, got: {e}"),
                Ok(data) => panic!("host: ctrl_send_drop_stall expected Stall, but got {} bytes", data.len()),
            }

            // Verify the device still works after the implicit stall.
            let echo_data: Vec<u8> = (0..8).collect();
            host_control_out(&_intf, if_num, ctrl_req::ECHO, &echo_data)
                .expect("host: post-drop echo out failed");
            let reply = host_control_in(&_intf, if_num, ctrl_req::ECHO, echo_data.len() as u16)
                .expect("host: post-drop echo in failed");
            assert_eq!(reply.as_slice(), echo_data.as_slice(), "post-drop echo mismatch");
            println!("host: post-drop echo OK");

            host_stop(&_intf, if_num);
        });
    });

    thread::sleep(Duration::from_millis(500));
    reg.remove().unwrap();
}

/// Stress test: repeated drops in both directions with recovery verification.
///
/// This exercises `clear_prev_event()` across multiple iterations. After each
/// implicit STALL via Drop, we sleep to ensure the device has returned to
/// `event_timeout()` before sending the next request. This forces the next
/// `event()` call to go through `clear_prev_event()` with a potentially stale
/// `setup_event`, catching regressions in EL2HLT handling in both `do_halt()`
/// and `clear_prev_event()`.
#[test]
fn ctrl_drop_recovery_stress() {
    init();
    let _mutex = exclusive();

    if skip_host() {
        return;
    }

    let (reg, mut custom, _ep_rx, _ep_tx) =
        setup_gadget_with_id(VID, PID, "ctrl drop stress test", "ctrl-drop-stress-001");

    let stop = Arc::new(AtomicBool::new(false));
    let stop_dev = stop.clone();

    thread::scope(|s| {
        s.spawn(move || run_device_ctrl_events(&mut custom, &stop_dev));

        s.spawn(|| {
            let (_intf, _ep_in, _ep_out, if_num) = open_host_device(VID, PID);

            let rounds = 5;

            for round in 0..rounds {
                println!("host: stress round {}/{rounds}", round + 1);

                // ── Drop a CtrlReceiver (HostToDevice direction) ──
                let result = host_control_out(&_intf, if_num, ctrl_req::DROP_RECV, &[0xAA; 16]);
                match &result {
                    Err(TransferError::Stall) => {}
                    other => panic!("round {round}: DROP_RECV expected Stall, got: {other:?}"),
                }

                // Wait so the device returns to event_timeout() before the next
                // request arrives. This ensures clear_prev_event() runs with the
                // stale setup_event (if do_halt in Drop failed to clear it).
                thread::sleep(Duration::from_millis(50));

                // Verify recovery with two consecutive echo round-trips.
                // Two rounds ensure clear_prev_event runs for both the first
                // post-drop event AND a subsequent normal event.
                for echo_idx in 0..2u8 {
                    let echo_data: Vec<u8> = (0..16).map(|b| b ^ echo_idx).collect();
                    host_control_out(&_intf, if_num, ctrl_req::ECHO, &echo_data).unwrap_or_else(|e| {
                        panic!("round {round}: post-recv-drop echo[{echo_idx}] out failed: {e}")
                    });
                    let reply = host_control_in(&_intf, if_num, ctrl_req::ECHO, echo_data.len() as u16)
                        .unwrap_or_else(|e| {
                            panic!("round {round}: post-recv-drop echo[{echo_idx}] in failed: {e}")
                        });
                    assert_eq!(reply, echo_data, "round {round}: post-recv-drop echo[{echo_idx}] mismatch");
                }

                // ── Drop a CtrlSender (DeviceToHost direction) ──
                let result = host_control_in(&_intf, if_num, ctrl_req::DROP_SEND, 64);
                match &result {
                    Err(TransferError::Stall) => {}
                    other => panic!("round {round}: DROP_SEND expected Stall, got: {other:?}"),
                }

                thread::sleep(Duration::from_millis(50));

                for echo_idx in 0..2u8 {
                    let echo_data: Vec<u8> = (0..16u8).map(|b| b.wrapping_add(echo_idx)).collect();
                    host_control_out(&_intf, if_num, ctrl_req::ECHO, &echo_data).unwrap_or_else(|e| {
                        panic!("round {round}: post-send-drop echo[{echo_idx}] out failed: {e}")
                    });
                    let reply = host_control_in(&_intf, if_num, ctrl_req::ECHO, echo_data.len() as u16)
                        .unwrap_or_else(|e| {
                            panic!("round {round}: post-send-drop echo[{echo_idx}] in failed: {e}")
                        });
                    assert_eq!(reply, echo_data, "round {round}: post-send-drop echo[{echo_idx}] mismatch");
                }
            }

            println!("host: all {rounds} stress rounds passed");
            host_stop(&_intf, if_num);
        });
    });

    thread::sleep(Duration::from_millis(500));
    reg.remove().unwrap();
}
