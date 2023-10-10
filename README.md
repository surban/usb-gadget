usb-gadget
==========

[![crates.io page](https://img.shields.io/crates/v/usb-gadget)](https://crates.io/crates/usb-gadget)
[![docs.rs page](https://docs.rs/usb-gadget/badge.svg)](https://docs.rs/usb-gadget)
[![Apache 2.0 license](https://img.shields.io/crates/l/usb-gadget)](https://github.com/surban/usb-gadget/blob/master/LICENSE)

This library allows implementation of USB peripherals, so called **USB gadgets**,
on Linux devices that have a USB device controller (UDC).
Both, pre-defined USB functions and fully custom implementations of the USB
interface are supported.

The following pre-defined USB functions, implemented by kernel drivers, are available:

* network interface
    * CDC ECM
    * CDC ECM (subset)
    * CDC EEM
    * CDC NCM
    * RNDIS
* serial port
    * CDC ACM
    * generic
* human interface device (HID)
* mass-storage device (MSD)

In addition fully custom USB functions can be implemented in user-mode Rust code.

Support for OS-specific descriptors and WebUSB is also provided.

Features
--------

This crate provides the following optional features:

* `tokio`: enables async support for custom USB functions on top of the Tokio runtime.

Requirements
------------

The minimum support Rust version (MSRV) is 1.73.

A USB device controller (UDC) supported by Linux is required. Normally, standard
PCs *do not* include an UDC.
A Raspberry Pi 4 contains an UDC, which is connected to its USB-C port.

The following Linux kernel configuration options should be enabled for full functionality:

  * `CONFIG_USB_GADGET`
  * `CONFIG_USB_CONFIGFS`
  * `CONFIG_USB_CONFIGFS_SERIAL`
  * `CONFIG_USB_CONFIGFS_ACM`
  * `CONFIG_USB_CONFIGFS_NCM`
  * `CONFIG_USB_CONFIGFS_ECM`
  * `CONFIG_USB_CONFIGFS_ECM_SUBSET`
  * `CONFIG_USB_CONFIGFS_RNDIS`
  * `CONFIG_USB_CONFIGFS_EEM`
  * `CONFIG_USB_CONFIGFS_MASS_STORAGE`
  * `CONFIG_USB_CONFIGFS_F_FS`
  * `CONFIG_USB_CONFIGFS_F_HID`

root permissions are required to configure USB gadgets on Linux and
the `configfs` filesystem needs to be mounted.


License
-------

usb-gadget is licensed under the [Apache 2.0 license].

[Apache 2.0 license]: https://github.com/surban/usb-gadget/blob/master/LICENSE

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in usb-gadget by you, shall be licensed as Apache 2.0, without any
additional terms or conditions.
