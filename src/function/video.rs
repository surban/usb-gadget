//! USB Video Class (UVC) function.
//!
//! The Linux kernel configuration option `CONFIG_USB_CONFIGFS_F_UVC` must be enabled. It must be paired with a userspace program that responds to UVC control requests and fills buffers to be queued to the V4L2 device that the driver creates. For example https://gitlab.freedesktop.org/camera/uvc-gadget.
//!
//! # Example
//!
//! ```no_run
//! use usb_gadget::function::video::{Uvc, Frame, Format};
//! use usb_gadget::{default_udc, Class, Config, Gadget, Id, Strings};
//!
//! // Create a new UVC function with the specified frames:
//! // - 640x360 YUYV format at 15, 30, 60, 120 fps
//! // - 640x360 MJPEG format at 15, 30, 60, 120 fps
//! // - 1280x720 MJPEG format at 30, 60 fps
//! // - 1920x1080 MJPEG format at 30 fps
//! let (video, func) = Uvc::new(vec![
//!     Frame::new(640, 360, vec![15, 30, 60, 120], Format::Yuyv),
//!     Frame::new(640, 360, vec![15, 30, 60, 120], Format::Mjpeg),
//!     Frame::new(1280, 720, vec![30, 60], Format::Mjpeg),
//!     Frame::new(1920, 1080, vec![30], Format::Mjpeg),
//! ]).build();
//!
//! let udc = default_udc().expect("cannot get UDC");
//! let reg =
//!     // USB device descriptor base class 0xEF, 0x02, 0x01: Misc, Interface Association Descriptor
//!     // Linux Foundation VID Gadget PID
//!     Gadget::new(Class::new(0xEF, 0x02, 0x01), Id::new(0x1d6b, 0x0104), Strings::new("Clippy Manufacturer", "Rust Video Device", "RUST0123456"))
//!         .with_config(Config::new("UVC Config 1").with_function(func))
//!         .bind(&udc)
//!         .expect("cannot bind to UDC");
//!
//! println!(
//!     "UAC2 video {} at {} to {} status {:?}",
//!     reg.name().to_string_lossy(),
//!     reg.path().display(),
//!     udc.name().to_string_lossy(),
//!     video.status()
//! );
//! ```
//! The gadget will bind won't enumaterate with host unless a userspace program (such as uvc-gadget) is running and responding to UVC control requests.
use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
    fs,
    io::{Error, ErrorKind, Result},
    path::{Path, PathBuf},
};

use super::{
    util::{FunctionDir, Status},
    Function, Handle,
};

pub(crate) fn driver() -> &'static OsStr {
    OsStr::new("uvc")
}

/// USB Video Class (UVC) frame format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Format {
    /// YUYV format [Packed YUV formats](https://docs.kernel.org/6.12/userspace-api/media/v4l/pixfmt-packed-yuv.html). Currently only uncompressed format supported.
    Yuyv,
    /// MJPEG compressed format.
    Mjpeg,
}

impl Format {
    fn all() -> &'static [Format] {
        &[Format::Yuyv, Format::Mjpeg]
    }

    fn dir_name(&self) -> &'static OsStr {
        match self {
            Format::Yuyv => OsStr::new("yuyv"),
            Format::Mjpeg => OsStr::new("mjpeg"),
        }
    }

    fn group_dir_name(&self) -> &'static OsStr {
        match self {
            Format::Yuyv => OsStr::new("uncompressed"),
            _ => self.dir_name(),
        }
    }

    fn group_path(&self) -> PathBuf {
        format!("streaming/{}/{}", self.group_dir_name().to_string_lossy(), self.dir_name().to_string_lossy())
            .into()
    }

    fn header_link_path(&self) -> PathBuf {
        format!("streaming/header/h/{}", self.dir_name().to_string_lossy()).into()
    }

    fn color_matching_path(&self) -> PathBuf {
        format!("streaming/color_matching/{}", self.dir_name().to_string_lossy()).into()
    }

    fn color_matching_link_path(&self) -> PathBuf {
        self.group_path().join("color_matching")
    }
}

/// Frame color matching information properties.
///
/// It’s possible to specify some colometry information for each format you
/// create. This step is optional, and default information will be included if
/// this step is skipped; those default values follow those defined in the
/// Color Matching Descriptor section of the UVC specification.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct ColorMatching {
    /// Color primaries
    pub color_primaries: u8,
    /// Transfer characteristics
    pub transfer_characteristics: u8,
    /// Matrix coefficients
    pub matrix_coefficients: u8,
}

impl ColorMatching {
    /// Create a new color matching information with the specified properties.
    pub fn new(color_primaries: u8, transfer_characteristics: u8, matrix_coefficients: u8) -> Self {
        Self { color_primaries, transfer_characteristics, matrix_coefficients }
    }
}

/// Helper to create a new [`UvcFrame`].
#[derive(Debug, Clone)]
pub struct Frame {
    /// Frame width in pixels
    pub width: u32,
    /// Frame height in pixels
    pub height: u32,
    /// Frame [`Format`]
    pub format: Format,
    /// Frames per second available
    pub fps: Vec<u16>,
}

impl Frame {
    /// Create a new [`UvcFrame`] with the specified properties.
    pub fn new(width: u32, height: u32, fps: Vec<u16>, format: Format) -> Self {
        Self { width, height, format, fps }
    }
}

impl From<Frame> for UvcFrame {
    fn from(frame: Frame) -> Self {
        UvcFrame {
            width: frame.width,
            height: frame.height,
            intervals: frame.fps.iter().map(|i| (1_000_000_000 / *i as u32)).collect(),
            color_matching: None,
            format: frame.format,
        }
    }
}

/// USB Video Class (UVC) frame configuration.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct UvcFrame {
    /// Frame width in pixels
    pub width: u32,
    /// Frame height in pixels
    pub height: u32,
    /// Frame intervals available each in 100 ns units
    pub intervals: Vec<u32>,
    /// Color matching information. If not provided, the default values are used.
    pub color_matching: Option<ColorMatching>,
    /// Frame format
    pub format: Format,
}

impl UvcFrame {
    fn dir_name(&self) -> String {
        format!("{}p", self.height)
    }

    fn path(&self) -> PathBuf {
        self.format.group_path().join(&self.dir_name())
    }

    /// Create a new UVC frame with the specified properties.
    pub fn new(width: u32, height: u32, format: Format, intervals: Vec<u32>) -> Self {
        Self { width, height, intervals, color_matching: None, format }
    }
}

/// Builder for USB Video Class (UVC) function. None value uses the f_uvc default/generated value.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct UvcBuilder {
    /// Interval for polling endpoint for data transfers
    pub streaming_interval: Option<u8>,
    /// bMaxBurst for super speed companion descriptor. Valid values are 1-15.
    pub streaming_max_burst: Option<u8>,
    /// Maximum packet size this endpoint is capable of sending or receiving when this configuration is selected. Valid values are 1024/2048/3072.
    pub streaming_max_packet: Option<u32>,
    /// Video device interface name
    pub function_name: Option<String>,
    /// Video frames available
    pub frames: Vec<UvcFrame>,
    /// Processing Unit's bmControls field
    pub processing_controls: Option<u8>,
    /// Camera Terminal's bmControls field
    pub camera_controls: Option<u8>,
}

impl UvcBuilder {
    /// Build the USB function.
    ///
    /// The returned handle must be added to a USB gadget configuration.
    pub fn build(self) -> (Uvc, Handle) {
        let dir = FunctionDir::new();
        (Uvc { dir: dir.clone() }, Handle::new(UvcFunction { builder: self, dir }))
    }
}

#[derive(Debug)]
struct UvcFunction {
    builder: UvcBuilder,
    dir: FunctionDir,
}

impl Function for UvcFunction {
    fn driver(&self) -> OsString {
        driver().into()
    }

    fn dir(&self) -> FunctionDir {
        self.dir.clone()
    }

    fn register(&self) -> Result<()> {
        if self.builder.frames.is_empty() {
            return Err(Error::new(ErrorKind::InvalidInput, "at least one frame must exist"));
        }

        // format groups to link to header
        let mut formats_to_link: HashSet<Format> = HashSet::new();

        // create frame descriptors
        for frame in &self.builder.frames {
            self.dir.create_dir_all(frame.path())?;
            self.dir.write(frame.path().join("wWidth"), frame.width.to_string())?;
            self.dir.write(frame.path().join("wHeight"), frame.height.to_string())?;
            self.dir.write(
                frame.path().join("dwMaxVideoFrameBufferSize"),
                (frame.width * frame.height * 2).to_string(),
            )?;
            self.dir.write(
                frame.path().join("dwFrameInterval"),
                frame.intervals.iter().map(|i| i.to_string()).collect::<Vec<String>>().join("\n"),
            )?;
            formats_to_link.insert(frame.format);

            if let Some(color_matching) = frame.color_matching.as_ref() {
                let color_matching_path = frame.format.color_matching_path();
                // can only have one color matching information per format
                if !color_matching_path.is_dir() {
                    self.dir.create_dir_all(&color_matching_path)?;
                    self.dir.write(
                        frame.format.color_matching_path().join("bColorPrimaries"),
                        color_matching.color_primaries.to_string(),
                    )?;
                    self.dir.write(
                        frame.format.color_matching_path().join("bTransferCharacteristics"),
                        color_matching.transfer_characteristics.to_string(),
                    )?;
                    self.dir.write(
                        frame.format.color_matching_path().join("bMatrixCoefficients"),
                        color_matching.matrix_coefficients.to_string(),
                    )?;
                    self.dir.symlink(&color_matching_path, frame.format.color_matching_link_path())?;
                } else {
                    log::warn!("Color matching information already exists for format {:?}", frame.format);
                }
            }
        }

        // header linking format descriptors and associated frames to header after creating
        // otherwise cannot add new frames
        self.dir.create_dir_all("streaming/header/h")?;
        self.dir.create_dir_all("control/header/h")?;

        for format in formats_to_link {
            self.dir.symlink(format.group_path(), format.header_link_path())?;
        }

        // supported speeds: all linked but selected based on gadget speed: https://github.com/torvalds/linux/blob/master/drivers/usb/gadget/function/f_uvc.c#L732
        self.dir.symlink("streaming/header/h", "streaming/class/fs/h")?;
        self.dir.symlink("streaming/header/h", "streaming/class/hs/h")?;
        self.dir.symlink("streaming/header/h", "streaming/class/ss/h")?;
        self.dir.symlink("control/header/h", "control/class/fs/h")?;
        self.dir.symlink("control/header/h", "control/class/ss/h")?;

        // controls
        if let Some(processing_controls) = self.builder.processing_controls {
            self.dir.write("control/processing/default/bmControls", processing_controls.to_string())?;
        }

        // terminal
        if let Some(camera_controls) = self.builder.camera_controls {
            self.dir.write("control/terminal/camera/default/bmControls", camera_controls.to_string())?;
        }

        // bandwidth configuration
        if let Some(interval) = self.builder.streaming_interval {
            self.dir.write("streaming_interval", interval.to_string())?;
        }
        if let Some(max_burst) = self.builder.streaming_max_burst {
            self.dir.write("streaming_maxburst", max_burst.to_string())?;
        }
        if let Some(max_packet) = self.builder.streaming_max_packet {
            self.dir.write("streaming_maxpacket", max_packet.to_string())?;
        }

        Ok(())
    }
}

/// USB Video Class (UVC) function.
#[derive(Debug)]
pub struct Uvc {
    dir: FunctionDir,
}

impl Uvc {
    /// Creates a new USB Video Class (UVC) builder with f_uvc video defaults.
    pub fn builder() -> UvcBuilder {
        UvcBuilder::default()
    }

    /// Creates a new USB Video Class (UVC) with the specified frames.
    pub fn new<F>(frames: Vec<F>) -> UvcBuilder
    where
        UvcFrame: From<F>,
    {
        let frames = frames.into_iter().map(UvcFrame::from).collect();
        UvcBuilder { frames, ..Default::default() }
    }

    /// Access to registration status.
    pub fn status(&self) -> Status {
        self.dir.status()
    }
}

fn remove_class_headers<P: AsRef<Path>>(path: P) -> Result<()> {
    for entry in fs::read_dir(path)? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        let header_path = path.join("h");
        if header_path.is_symlink() {
            log::trace!("removing UVC header {:?}", path);
            fs::remove_file(header_path)?;
        }
    }

    Ok(())
}

pub(crate) fn remove_handler(dir: PathBuf) -> Result<()> {
    // remove header links for control and streaming
    let ctrl_class = dir.join("control/class");
    if ctrl_class.is_dir() {
        remove_class_headers(ctrl_class)?;
    }
    let stream_class = dir.join("streaming/class");
    if stream_class.is_dir() {
        remove_class_headers(stream_class)?;
    }

    // remove all UVC frames, color matching information and header links
    if dir.join("streaming").is_dir() {
        for format in Format::all() {
            // remove header link first to allow removing frames
            let header_link_path = dir.join(format.header_link_path());
            if header_link_path.is_symlink() {
                log::trace!("removing UVC header link {:?}", header_link_path);
                fs::remove_file(header_link_path)?;
            }

            let color_matching_dir = dir.join(format.color_matching_path());
            if color_matching_dir.is_dir() {
                log::trace!("removing UVC color matching information {:?}", color_matching_dir);
                fs::remove_file(dir.join(format.color_matching_link_path()))?;
                fs::remove_dir(color_matching_dir)?;
            }

            let group_dir = dir.join(format.group_path());
            if group_dir.is_dir() {
                for entry in fs::read_dir(&group_dir)? {
                    let Ok(entry) = entry else { continue };
                    let path = entry.path();
                    if path.is_dir() && !path.is_symlink() {
                        log::trace!("removing UVC frame {:?}", path);
                        fs::remove_dir(path)?;
                    }
                }

                log::trace!("removing UVC group {:?}", group_dir);
                fs::remove_dir(group_dir)?;
            }
        }
    }

    // finally remove header folders
    let stream_header = dir.join("streaming/header/h");
    if stream_header.is_dir() {
        fs::remove_dir(stream_header)?;
    }
    let control_header = dir.join("control/header/h");
    if control_header.is_dir() {
        fs::remove_dir(control_header)?;
    }

    Ok(())
}
