mod common;
use common::*;

use usb_gadget::function::video::{Uvc, ColorMatching, Format, Frame};

#[test]
fn video() {
    init();

    let mut builder = Uvc::new(vec![
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

    unreg(reg).unwrap();
}
