mod common;
use common::*;

use usb_gadget::function::video::{ColorMatching, Format, Frame, Uvc, UvcFrame};

#[test]
fn video() {
    init();
    let _mutex = exclusive();

    let mut builder = Uvc::builder().with_frames(vec![
        Frame::new(640, 360, vec![15, 30, 60, 120], Format::Yuyv),
        Frame::new(640, 360, vec![15, 30, 60, 120], Format::Mjpeg),
        Frame::new(1280, 720, vec![30, 60], Format::Mjpeg),
        Frame::new(1920, 1080, vec![30], Format::Mjpeg),
    ]);
    builder.frames[0].color_matching = Some(ColorMatching::new(0x4, 0x1, 0x2));
    builder.processing_controls = Some(0x05);
    builder.camera_controls = Some(0x60);
    let (video, func) = builder.build();
    let reg = reg(func);

    println!("UVC video device at {}", video.status().path().unwrap().display());

    // NOTE: UVC gadgets do not enumerate on the host without a user-space
    // streaming daemon, so host-side verification is skipped here.

    unreg(reg).unwrap();
}

#[test]
fn video_framebased_nv12() {
    init();
    let _mutex = exclusive();

    let builder = Uvc::builder().with_frames(vec![
        UvcFrame::new(640, 480, Format::nv12(), [333333, 500000]),
        UvcFrame::new(1280, 720, Format::nv12(), [333333]),
    ]);
    let (video, func) = builder.build();
    let reg = reg(func);

    println!("UVC NV12 framebased video device at {}", video.status().path().unwrap().display());

    unreg(reg).unwrap();
}

#[test]
fn video_framebased_h264() {
    init();
    let _mutex = exclusive();

    let builder = Uvc::builder().with_frames(vec![UvcFrame::new(1920, 1080, Format::h264(), [333333])]);
    let (video, func) = builder.build();
    let reg = reg(func);

    println!("UVC H264 framebased video device at {}", video.status().path().unwrap().display());

    unreg(reg).unwrap();
}
