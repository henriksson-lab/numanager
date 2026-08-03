# Euresys eGrabber frame grabbers

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::egrabber_framegrabber` |
| Families | Euresys eGrabber GenTL producers and attached remote camera ports |
| Support level | Configured producer package boundary, CTI file/digest/ABI checks, and default-off GenTL interface/device inventory through the optional SDK backend |
| Protocol evidence | Manufacturer eGrabber 26.07.0.5 SDK/source package, `GenTL_v1_6.h`, Euresys GenTL headers, CTI producers, and Linux PCI driver source |
| Transport | Configured GenTL producer package and optional SDK inventory backend behind `egrabber-sdk` plus `load_sdk = true` |
| Discovery | `HardwareConfig` only; no automatic hardware scan |
| Validation | Package/file/ABI inspection only in this repository; hardware bench coverage has not been recorded |
| Runtime/evidence notes | Capture, streaming, producer updates, reset, firmware, and maintenance operations are hidden from regular and advanced command surfaces |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| Configured framegrabber label | `framegrabber`, `gentl.producer`, `pci` | Owns one configured GenTL producer resource |
| Framegrabber camera port | `camera_port`, `gentl.remote_device` | Reports remote camera binding inventory from the producer resource |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `<label> GenTL producer` | `gentl_producer` | Records configured SDK root, CTI producer path, SHA-256 digest status, ABI status, package strategy, and optional SDK inventory state |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `GenTLInventory` | Framegrabber | `None` or `GenericCommandRequest` that is not hidden maintenance | Map of SDK state, producer status, interface count, device count, interface IDs, and device IDs | Runtime token completion after configured file/ABI checks and optional SDK inventory call | Not sequenceable |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `model` | Framegrabber | `String` | none | R | configured string | No | Configured package metadata |
| `serial_number` | Framegrabber | `String` | none | R | configured string or null | No | Configured package metadata |
| `transport` | Framegrabber | `String` | none | R | configured string | No | Configured package metadata |
| `sdk_root` | Framegrabber | `String` | none | R | configured path or null | No | Explicit SDK package boundary |
| `producer_path` | Framegrabber | `String` | none | R | configured path or null | No | Explicit CTI producer file |
| `producer_sha256` | Framegrabber | `String` | none | R | configured digest or null | No | Expected CTI producer digest |
| `load_sdk` | Framegrabber | `Bool` | none | R | configured boolean | No | Explicit SDK backend gate |
| `sdk_state` | Framegrabber | `String` | none | R | SDK backend state | No | Optional SDK backend state |
| `producer_file_status` | Framegrabber | `String` | none | R | configured file inspection result | No | CTI producer path check |
| `producer_file_size` | Framegrabber | `ByteCount` | bytes | R | file size or null | No | CTI producer file metadata |
| `producer_digest_state` | Framegrabber | `String` | none | R | digest match/mismatch/unavailable | No | SHA-256 verification |
| `producer_abi_state` | Framegrabber | `String` | none | R | ABI symbol check result | No | GenTL producer ABI inspection |
| `gentl_probe_state` | Framegrabber | `String` | none | R | SDK inventory state | No | Optional SDK inventory backend |
| `gentl_interface_count` | Framegrabber | `I64` | count | R | discovered or zero | No | Optional SDK inventory backend |
| `gentl_device_count` | Framegrabber | `I64` | count | R | discovered or zero | No | Optional SDK inventory backend |
| `gentl_interfaces` | Framegrabber | `List` | none | R | discovered interface IDs | No | Optional SDK inventory backend |
| `gentl_devices` | Framegrabber | `List` | none | R | discovered device IDs | No | Optional SDK inventory backend |
| `support_level` | Framegrabber | `String` | none | R | static support summary | No | Driver metadata |
| `capture_gate` | Framegrabber | `String` | none | R | static support summary | No | Driver metadata |
| `stream_gate` | Framegrabber | `String` | none | R | static support summary | No | Driver metadata |
| `package_strategy` | Framegrabber | `String` | none | R | third-party package policy | No | Driver metadata |
| `third_party_notice` | Framegrabber | `String` | none | R | package boundary notice | No | Driver metadata |
| `hardware_validated` | Framegrabber | `Bool` | none | R | false until a bench note is recorded | No | Driver metadata |
| `bound_framegrabber` | Camera port | `String` | none | R | parent label | No | Binding inventory |
| `remote_device_count` | Camera port | `I64` | count | R | discovered or zero | No | Optional SDK inventory backend |
| `remote_devices` | Camera port | `List` | none | R | discovered device IDs | No | Optional SDK inventory backend |
| `camera_binding_state` | Camera port | `String` | none | R | binding summary | No | Optional SDK inventory backend |
| `capture_available` | Camera port | `Bool` | none | R | false | No | Public capture surface status |
| `stream_available` | Camera port | `Bool` | none | R | false | No | Public stream surface status |
| `hardware_validated` | Camera port | `Bool` | none | R | false until a bench note is recorded | No | Driver metadata |

## Evidence Gate

The driver records the third-party SDK/package boundary and checks configured
producer files only through explicit configuration. The package itself is
third-party excluded data unless redistribution terms are recorded for the exact
package being shipped. Record upstream package/version, platform, CTI file
identity, SHA-256 digest, and license status before distributing a package.

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- discover_devices` | Configured discovery output when an eGrabber device is present in `HardwareConfig` |

## Configuration

```toml
[[devices]]
id = 1000
label = "Euresys eGrabber"
driver = "egrabber_framegrabber"
property.model = "Coaxlink/Grablink eGrabber GenTL producer"
property.transport = "PCI/GenTL"
property.sdk_root = "/opt/euresys/egrabber"
property.producer_path = "/opt/euresys/egrabber/lib/x86_64/coaxlink.cti"
property.producer_sha256 = "sha256:<64 hex digits>"
property.load_sdk = false
```

Use the driver crate feature `egrabber-sdk` to enable ABI probing and live GenTL
inventory. Leaving `load_sdk = false` keeps the producer file unloaded.

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Record framegrabber model, serial/PCI identity, SDK version, CTI producer identity, operating system, driver version, interface inventory, remote-device inventory, and matching runtime output on real hardware |
| Capture | Add typed acquisition after GenTL buffer allocation, queue, start, wait, underrun/error, timestamp, payload, and teardown behavior are recorded against SDK/API evidence and real hardware |
| Streaming | Add streaming after continuous buffer ownership, cancellation, backpressure, frame loss, and safe stop behavior are recorded |
| Maintenance boundary | Keep producer updates, firmware actions, resets, and package mutation hidden unless a future explicit maintenance workflow is designed and separately validated |

## Unblock Trace Checklist

| Item | Record |
| --- | --- |
| Hardware identity | Board model, serial/PCI IDs, firmware if reported, host platform |
| Software identity | eGrabber package version, CTI producer path, file size, SHA-256 digest, driver/kernel package versions |
| Inventory behavior | Interface count/IDs, device count/IDs, empty-bus behavior, multi-camera behavior |
| Error behavior | Missing CTI file, digest mismatch, ABI failure, SDK load failure, no-camera inventory |
| Runtime output | `discover_devices` descriptors, resource metadata, `GenTLInventory` response, and any SDK logs needed to interpret failures |
