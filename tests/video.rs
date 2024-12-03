mod common;
use common::*;

use usb_gadget::function::video::{Uvc, UvcFormat, UvcColorMatching};

#[test]
fn video() {
    init();

    let mut builder = Uvc::new(vec![
        (640, 480, UvcFormat::Yuyv),
        (640, 480, UvcFormat::Mjpeg),
        (1280, 720, UvcFormat::Mjpeg),
        (1920, 1080, UvcFormat::Mjpeg),
    ]);
    builder.frames[0].color_matching = Some(UvcColorMatching::new(0x4, 0x1, 0x2));
    builder.processing_controls = Some(0x05);
    builder.camera_controls = Some(0x60);
    let (video, func) = builder.build();
    let reg = reg(func);

    println!("UVC video device at {}", video.status().path().unwrap().display());

    unreg(reg).unwrap();
}
