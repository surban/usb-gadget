# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog],
and this project adheres to [Semantic Versioning].

## 1.1.0 - 2026-03-12
### Added
- constants for common classes
- constants for testing vid/pid

## 1.0.0 - 2026-03-11
### Added
- USB gadget CLI tool
- DMAbuf support for custom functions
- UAC1 gadget support
- loopback gadget support
- sourcesink gadget support
- UVC: additional video formats
- custom function: DFU descriptor support
- custom name support for gadgets
- device path lookup for gadgets (HID)
- `RegGadget::functions()` for querying registered functions
- UDC driver name querying
- `UsbVersion::V21` variant for USB 2.0 with BOS descriptor support
### Changed
- reversed `Speed` enum ordering for meaningful comparisons
### Fixed
- `DeviceInterfaceGUID` missing braces around GUID
- custom function: panic in `recv_and_fetch_timeout`
- RNDIS network function: write interface class values without `0x` prefix
  as expected by kernel
- stable and deduplicated function ordering in gadget configurations
### Removed
- deprecated `Config::set_max_power_ma()` method


## 0.7.6 - 2025-10-10
### Changed
- preserve functions order in gadget configuration by Warren Campbell


## 0.7.5 - 2024-12-06
### Added
- Printer gadget support by John Whittington
- UAC2 gadget support by John Whittington
- UVC gadget support by John Whittington


## 0.7.4 - 2024-11-29
### Added
- MIDI gadget support by John Whittington


## 0.7.3 - 2024-11-19
### Added
- custom descriptor support


## 0.7.2 - 2024-06-25
### Changed
- clarify meaning of BCD
### Fixed
- handle ordering


## 0.7.1 - 2024-04-22
### Fixed
- maximum device current handling
- use correct bcdVersion in descriptor


## 0.7.0 - 2024-03-13
### Fixed
- device number data types


## 0.6.0 - 2023-11-11
### Changed
- custom interface: make status() return an Option<_> 


## 0.5.2 - 2023-11-09
### Changed
- use old value for bcdVersion of OS descriptors for compatibility
  with older Linux kernels


## 0.5.1 - 2023-11-09
### Added
- interface-specific device class


## 0.5.0 - 2023-11-07
### Added
- custom interface: support usage with external USB gadget
  registration and pre-mounted FunctionFS
- custom interface: allow specification of FunctionFS
  mount options and skip of interface initialization
- allow gadget registration without binding
### Changed
- custom interface: switch to Bytes-based buffers


## 0.4.1 - 2023-11-01
### Added
- examples


## 0.4.0 - 2023-11-01
### Added
- extend Microsoft OS descriptor support


## 0.3.0 - 2023-10-13
### Added
- expose gadget registration status


## 0.2.0 - 2023-10-10
### Added
- custom USB function support


## 0.1.0 - 2023-09-29
- initial release
