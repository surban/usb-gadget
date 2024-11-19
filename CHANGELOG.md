# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog],
and this project adheres to [Semantic Versioning].

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
