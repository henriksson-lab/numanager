# Cypress EZ-USB loader and ReNumeration evidence

## Status

This note records public-source evidence for Cypress EZ-USB loader-stage USB
identity and firmware download mechanics. It is generic chip evidence, not
Andor, Lumenera, MCL, or other vendor identity evidence.

## Public sources

| Source | Evidence used |
| --- | --- |
| Cypress/Infineon *EZ-USB FX2 Technical Reference Manual*, Chapter 3, "Enumeration and ReNumeration" | FX2 devices can enumerate before firmware is loaded; the default USB device can accept firmware over endpoint 0 and then electrically simulate disconnect/reconnect so firmware descriptors enumerate as a second USB identity |
| Cypress/Infineon FX2 Technical Reference Manual, firmware-load vendor request section | Vendor request `0xA0` writes internal RAM and `CPUCS` at `0xe600`; the normal loader sequence holds the 8051 in reset, writes firmware records, then releases reset |
| Local public-source audit notes in `/data/henriksson/github/claude/8051-tools/FX2_RENUMERATION.md` | Consolidates the FX2 TRM evidence and records that generic `04b4:8613` has no manufacturer/product/serial strings that can identify the downstream vendor before firmware runs |

## Discovery rule

The generic Cypress FX2 loader identity `04b4:8613` is not enough evidence to
claim a device as Andor, Lumenera, MCL, or any other vendor device. Passive USB
discovery may report it only as an ambiguous EZ-USB pre-firmware loader.

Vendor-specific classification needs one of:

- a vendor-specific loader VID/PID or descriptor with recorded provenance;
- user configuration that explicitly selects the expected driver and device;
- an active, side-effecting firmware probe that observes the post-firmware
  VID/PID; or
- hardware validation notes or captured USB traces identifying the device.

## Implementation boundary

`numanager_drivers::ez_usb` exposes ambiguous loader candidates with metadata
such as `usb_stage = "loader"` and
`usb_identity_confidence = "ambiguous"`. It exposes no camera, stage, motion, or
other hardware operation.

Driver-specific firmware upload remains a hidden initialization step behind
explicit configuration, package identity, and digest checks. A successful upload
does not by itself prove the vendor identity; the driver must observe the
post-firmware USB identity or report an error.
