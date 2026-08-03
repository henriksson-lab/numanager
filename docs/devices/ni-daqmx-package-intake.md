# NI-DAQmx Package Intake

This note records local NI-DAQmx installer/package identities for the optional
`numanager-imswitch-daqmx` backend. It is package evidence only. It does not
prove DAQmx task behavior, trigger routing, physical channel validity, or
hardware completion semantics.

## Local Inputs

| Item | Value |
| --- | --- |
| Intake directory | `/home/mahogny/github/claude/reveng-dll/nidaq/` |
| Linux driver archive | `NILinux2026Q3DeviceDrivers.zip` |
| Linux archive SHA-256 | `89c4a38fe6019e0791597646d060d66b492dd64cfd3fa3a1f6ff9eb5b8806296` |
| Linux archive bytes | 460773 |
| Windows online installer | `ni-daqmx_26.5_online.exe` |
| Windows installer SHA-256 | `ee022e11ddc6d2132130f94c8609435a55026bc815453cca020f23ee683368a2` |
| Windows installer bytes | 9004840 |
| License / redistribution boundary | Linux package license files are identified below; the Windows online-installer payload has been inventoried but does not expose standalone license files at the inspected payload level; treat installers and installed SDK/runtime files as user-provided third-party excluded data unless project legal review records that redistribution is permitted for the exact files and use case |

## Linux Archive Contents

`unzip -l /home/mahogny/github/claude/reveng-dll/nidaq/NILinux2026Q3DeviceDrivers.zip`
reported these package entries:

| Bytes | Package |
| ---: | --- |
| 41433 | `ni-opensuse156-drivers-2026Q3.rpm` |
| 41434 | `ni-opensuse160-drivers-2026Q3.rpm` |
| 41246 | `ni-rhel10-drivers-2026Q3.rpm` |
| 41240 | `ni-rhel9-drivers-2026Q3.rpm` |
| 34152 | `ni-ubuntu2204-drivers-2026Q3.deb` |
| 34152 | `ni-ubuntu2404-drivers-2026Q3.deb` |
| 41459 | `ni-opensuse156-drivers-stream.rpm` |
| 41460 | `ni-opensuse160-drivers-stream.rpm` |
| 41273 | `ni-rhel10-drivers-stream.rpm` |
| 41262 | `ni-rhel9-drivers-stream.rpm` |
| 34170 | `ni-ubuntu2204-drivers-stream.deb` |
| 34172 | `ni-ubuntu2404-drivers-stream.deb` |

These are repository-enablement/driver installer packages, not the installed
SDK header inventory. The current Linux header evidence remains
`/usr/include/NIDAQmx.h` from the installed `libnidaqmx-devel` package, with the
digest recorded in [`ni-daqmx-sdk-api-audit.md`](ni-daqmx-sdk-api-audit.md).
That header audit records the 2003-2026 NI copyright banner, runtime-version
property IDs/getter symbols, `NIDAQmx.h count = 1`, the audited
`/usr/include/NIDAQmx.h` path, and no literal package-version macro, so the
Linux 26.5 package-input evidence below should not be treated as proof that the
installed header tree has been updated to 26.5 until a matching installed 26.5
header audit passes and records the target-platform `NIDAQmx.h` path and
digest. The header-audit script exits non-zero if `NIDAQmx.h` is absent from
the supplied file/directory.

## Linux Debian Package License Files

The package-input audit script now inspects `.deb` entries inside the Linux zip
when `dpkg-deb` is available. The Ubuntu repository-enable packages identify
themselves as version `26.5.0.49702-0+f550`, architecture `all`, maintained by
National Instruments, with homepage `https://www.ni.com/r/ni-linux-device-drivers`.
The audited packages are:

| Zip entry | Debian package |
| --- | --- |
| `ni-ubuntu2204-drivers-2026Q3.deb` | `ni-software-2026-jammy` |
| `ni-ubuntu2404-drivers-2026Q3.deb` | `ni-software-2026-noble` |
| `ni-ubuntu2204-drivers-stream.deb` | `ni-software-stream-jammy` |
| `ni-ubuntu2404-drivers-stream.deb` | `ni-software-stream-noble` |

Each audited Ubuntu package carries these license files under its
`/usr/share/doc/<package>/` payload directory:

| Bytes | SHA-256 | Path |
| ---: | --- | --- |
| 6954 | `78df09c4322c2161d711b4c2f14089f0edb22edb7641a96024be567eeaf27b88` | `IVI Foundation Inc License Agreement - English.txt` |
| 123438 | `ac4e491c05cfd2e461a33bddae3b511f0128abafffb572032d624628a7b4d932` | `NI Released License Agreement - English.txt` |

The NI license text is present in the packages and should be reviewed before
redistributing any NI installer, runtime, SDK, or installed binary/header file.
Until that review is recorded, numanager's default boundary is to load a
user-configured local NI installation behind `ni-daqmx-sdk` and not vendor NI
files in this repository.

## Linux RPM Package License Files

The package-input audit script also extracts `.rpm` entries inside the Linux zip
when `rpm2cpio` and `cpio` are available. On this host, `rpm` is not installed,
so RPM package metadata fields are not recorded, but the payload license-file
identities are repeatably audited.

Audited fixed-version RPM entries:

| Zip entry | License payload directory |
| --- | --- |
| `ni-opensuse156-drivers-2026Q3.rpm` | `usr/share/doc/ni-software-2026/` |
| `ni-opensuse160-drivers-2026Q3.rpm` | `usr/share/doc/ni-software-2026/` |
| `ni-rhel10-drivers-2026Q3.rpm` | `usr/share/doc/ni-software-2026/` |
| `ni-rhel9-drivers-2026Q3.rpm` | `usr/share/doc/ni-software-2026/` |

Audited stream RPM entries:

| Zip entry | License payload directory |
| --- | --- |
| `ni-opensuse156-drivers-stream.rpm` | `usr/share/doc/ni-software-stream/` |
| `ni-opensuse160-drivers-stream.rpm` | `usr/share/doc/ni-software-stream/` |
| `ni-rhel10-drivers-stream.rpm` | `usr/share/doc/ni-software-stream/` |
| `ni-rhel9-drivers-stream.rpm` | `usr/share/doc/ni-software-stream/` |

Each audited RPM payload carries these same license-file identities:

| Bytes | SHA-256 | Path |
| ---: | --- | --- |
| 6954 | `78df09c4322c2161d711b4c2f14089f0edb22edb7641a96024be567eeaf27b88` | `IVI Foundation Inc License Agreement - English.txt` |
| 123438 | `ac4e491c05cfd2e461a33bddae3b511f0128abafffb572032d624628a7b4d932` | `NI Released License Agreement - English.txt` |

## Windows 26.5 Boundary

The local Windows input is an online installer executable. The package-input
audit uses `7z` to inventory the PE metadata and extracted first-level
`NIPKG_PAYLOAD~` archive without vendoring NI files into this repository.

| Field | Value |
| --- | --- |
| PE type | `PE` |
| CPU | `x86` |
| Image version | `26.5` |
| File version | `26.5.0f145` |
| Product version | `26.5.0.49297` |
| Product name | `NI Package Manager` |
| Company name | `National Instruments Corporation` |
| Embedded payload | `.rsrc/1033/RCDATA/NIPKG_PAYLOAD`, `gzip`, 7022399 bytes |
| Extracted payload | `NIPKG_PAYLOAD~`, 18242560 bytes, `POSIX tar archive` |
| Extracted payload SHA-256 | `6f881a0ed343c34feee85aba5ecdeb02d20a8b21e9391d322ea490eff7f784c8` |

The extracted Windows payload contains these top-level files and localized
resource directories:

| Payload entry | Bytes |
| --- | ---: |
| `Install.exe` | 2221312 |
| `Install.exe.config` | 254 |
| `MIFSystemUtility64.dll` | 512288 |
| `NationalInstruments.LicenseManagement.Client.dll` | 54152 |
| `NationalInstruments.PackageManagement.Core.dll` | 240896 |
| `NationalInstruments.PackageManagement.Deployment.dll` | 167168 |
| `NationalInstruments.PackageManagement.Store.dll` | 91904 |
| `Newtonsoft.Json.dll` | 721320 |
| `niceiplib_STATIC_CRT.dll` | 175696 |
| `niMetaUtils.msm` | 1052672 |
| `nipkg.ini` | 352 |
| `nipkgclient.dll` | 12603720 |
| `de/Install.resources.dll` | 76544 |
| `fr/Install.resources.dll` | 77056 |
| `ja/Install.resources.dll` | 82176 |
| `ko/Install.resources.dll` | 77568 |
| `zh-CN/Install.resources.dll` | 71424 |

No standalone license, EULA, or copyright files were found in the extracted
first-level Windows installer payload. This does not establish legal
redistribution permission and does not replace a Windows installation/package
review. Before publishing a 26.5 Windows binding update, run the same
bindgen/header-audit procedure against an installed Windows NI header and
record:

- installed `NIDAQmx.h` path and SHA-256;
- combined header inventory SHA-256;
- generated `ni-daqmx-sys` source identity;
- platform and compiler target;
- runtime package/version reported by NI-DAQmx on Windows;
- license and redistribution boundary for the exact installed runtime/package
  files.

Do not infer Windows ABI compatibility from the Linux-generated bindings.

## Commands Used

```sh
scripts/audit-ni-daqmx-package-inputs.sh /home/mahogny/github/claude/reveng-dll/nidaq
sha256sum /home/mahogny/github/claude/reveng-dll/nidaq/NILinux2026Q3DeviceDrivers.zip /home/mahogny/github/claude/reveng-dll/nidaq/ni-daqmx_26.5_online.exe
file /home/mahogny/github/claude/reveng-dll/nidaq/NILinux2026Q3DeviceDrivers.zip /home/mahogny/github/claude/reveng-dll/nidaq/ni-daqmx_26.5_online.exe
unzip -l /home/mahogny/github/claude/reveng-dll/nidaq/NILinux2026Q3DeviceDrivers.zip
```

The audit script is the preferred repeatable intake command for installer files
or a directory of local package inputs. It records file digests, byte counts,
file types, zip archive entries, Debian package metadata when `dpkg-deb` is
available, RPM package metadata when `rpm` is available, embedded
license/copyright file identities for Debian/RPM packages when extraction tools
are available, Windows online-installer PE/payload inventory when `7z` is
available, standalone Windows payload license/EULA/copyright file identities
when present, and the evidence boundary. It does not replace installed-header
audits, bindgen-source audits, runtime probes, legal redistribution review, or
hardware validation.

## Next Evidence

- Complete legal review of the identified Linux license files and record whether
  any NI files may be redistributed for this project.
- Install or further extract the Windows package in an appropriate Windows
  environment and record the matching installed license files/terms.
- Audit installed Linux or Windows 26.5 SDK headers if those packages are used
  to regenerate bindings. The preferred procedure is to run
  `scripts/audit-ni-daqmx-sdk-headers.sh <installed-header-path-or-directory>`,
  regenerate the `ni-daqmx-sys` fork with its bindgen script for the target
  platform, push the fork, and then run
  `scripts/audit-ni-daqmx-sys-source.sh /home/mahogny/github/claude/ni-daqmx-sys`
  before updating numanager's git dependency revision. The scripts prefer an
  installed `bindgen` CLI and can fall back to the fork-local Cargo generator
  when the CLI is not installed.
- Re-run
  `scripts/audit-ni-daqmx-package-inputs.sh /home/mahogny/github/claude/reveng-dll/nidaq`
  whenever package files are added, removed, or replaced, then update this note
  with the new package identities.
- Re-run
  `scripts/audit-ni-daqmx-sys-source.sh /home/mahogny/github/claude/ni-daqmx-sys`
  after any bindgen regeneration and update
  [`ni-daqmx-sdk-api-audit.md`](ni-daqmx-sdk-api-audit.md) with the new fork
  revision and generated source hashes.
- Keep live task execution disabled until the bench checklist records runtime
  and hardware behavior.
