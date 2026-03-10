//! Async transfer test (recv_async / send_async with tokio).

use bytes::BytesMut;
use std::{sync::Arc, time::Duration};

use usb_gadget::function::custom::Event;

use super::*;

const VID: u16 = 0x1234;
const PID: u16 = 0x0020;

async fn run_device_async(
    mut custom: Custom, mut ep_rx: usb_gadget::function::custom::EndpointReceiver,
    mut ep_tx: usb_gadget::function::custom::EndpointSender,
) {
    use tokio::sync::Notify;

    let stop = Arc::new(Notify::new());

    let stop_rx = stop.clone();
    let rx_task = tokio::spawn(async move {
        let size = ep_rx.max_packet_size().unwrap();
        let mut expected = 0u8;
        loop {
            tokio::select! {
                result = ep_rx.recv_async(BytesMut::with_capacity(size)) => {
                    match result {
                        Ok(Some(data)) => {
                            assert!(
                                data.iter().all(|&x| x == expected),
                                "device async recv: expected all 0x{expected:02x}, got {:02x?}",
                                &data[..data.len().min(16)]
                            );
                            expected = expected.wrapping_add(1);
                        }
                        Ok(None) => {}
                        Err(e) => {
                            println!("device async recv stopped: {e}");
                            break;
                        }
                    }
                }
                _ = stop_rx.notified() => break,
            }
        }
    });

    let stop_tx = stop.clone();
    let tx_task = tokio::spawn(async move {
        let size = ep_tx.max_packet_size().unwrap().min(PACKET_SIZE);
        let mut b = 0u8;
        loop {
            let data = vec![b; size];
            tokio::select! {
                result = ep_tx.send_async(data.into()) => {
                    match result {
                        Ok(()) => b = b.wrapping_add(1),
                        Err(e) => {
                            println!("device async send stopped: {e}");
                            break;
                        }
                    }
                }
                _ = stop_tx.notified() => break,
            }
        }
    });

    // Event loop: handle control requests.
    let mut ctrl_data = Vec::new();
    let mut stopped = false;
    while !stopped {
        if custom.wait_event().await.is_err() {
            break;
        }
        match custom.event() {
            Ok(event) => match event {
                Event::SetupHostToDevice(req) => {
                    if req.ctrl_req().request == req::STOP {
                        stopped = true;
                    }
                    ctrl_data = req.recv_all().unwrap();
                }
                Event::SetupDeviceToHost(req) => {
                    if req.ctrl_req().request == req::ECHO {
                        req.send(&ctrl_data).unwrap();
                    }
                }
                _ => {}
            },
            Err(e) => {
                if !stopped {
                    panic!("device async event error: {e}");
                }
                break;
            }
        }
    }

    stop.notify_waiters();
    let _ = rx_task.await;
    let _ = tx_task.await;
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn transfer_async() {
    init();
    let _mutex = exclusive();

    if skip_host() {
        return;
    }

    let (vid, pid) = (VID, PID);
    let (reg, custom, ep_rx, ep_tx) = setup_gadget_with_id(vid, pid, "transfer test device", "transfer-test-001");

    let host = tokio::task::spawn_blocking(pipelined::run_host_pipelined);
    run_device_async(custom, ep_rx, ep_tx).await;
    host.await.unwrap();

    tokio::time::sleep(Duration::from_millis(500)).await;
    reg.remove().unwrap();
}
