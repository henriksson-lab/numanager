# WOSM MCU Command Surface

This note records the project-published WOSM MCU command page as the primary
source for the `numanager_drivers::wosm` text-command mappings.

## Evidence Identity

| Field | Value |
| --- | --- |
| Primary source | WOSM MCU command set |
| Firmware code base | `v0.900` |
| URL | <https://wosm.net/mcu/commands.php> |
| Micro-Manager note | WOSM support page says the bundled Micro-Manager `mmgr_dal_WOSM.dll` can be out of date and recommends replacing it with a newer DLL |
| Hardware validation | Not recorded in this repository |

The command page states that the browser interface talks to the controller using
plain text commands, and that the same command language is reachable from WML,
Micro-Manager scripts over Telnet, HTTP arguments, Ajax, a Telnet command line,
SPI A, and firmware-local calls.

## Transport

| Parameter | Source-backed value |
| --- | --- |
| User Telnet port | `23` |
| Driver/controller-PC Telnet port | `1023` |
| Command style | Plain text command plus arguments |
| Prompt completion | Existing driver evidence uses `W>` prompt completion |
| Line ending | Existing driver evidence uses CRLF |

The driver default is port `1023`, matching the command page's controller-PC
Telnet statement. Users can still configure `port = 23` for user-Telnet style
sessions.

## Documented Runtime Mappings

| Runtime operation | WOSM command |
| --- | --- |
| Digital switch/shutter output | `dig_out <val32> <mask32>` |
| Aggregate digital input read | `dig_in` |
| Digital line mode setup | `dig_mode <line> <mode>` |
| Light analog output write | `dac_dest p<s|t|u|v> <0..65535>` |
| Stage X/Y/Z move | `stg_out_x <newpos>`, `stg_out_y <newpos>`, `stg_out_z <newpos>` |
| Stage digital readback family | `stg_val_*`, `stg_dest_*`, `stg_min_*`, `stg_max_*` |
| DAC output readback | `dac_val`, `dac_dest`, `dac_out`, `dac_out_conf` |
| Accurate digital pulses | `dig_hilo`, `dig_lohi` |
| Macro timing | `pause`, `loop`, `dig_wait`, `fast`, `wml_run`, `wml_stop` |

`dac_out` is documented as read-only on the v0.900 command page, so writes must
use `dac_dest`. The driver maps public light percentages to unsigned 16-bit DAC
destination counts. Stage moves use the `stg_out_*` abstraction rather than
direct DAC writes so board configuration can choose motor, DAC, or PWM-backed
stage axes.

## Legacy Evidence Kept Separate

The existing WOSM driver still has source-backed legacy commands for:

| Legacy command | Current use |
| --- | --- |
| `P,<index>,<value>`, `N,<count>`, `R`, `E` | Switch-state timing sequence load/run/end |
| `A,<channel>` | Raw analog input readback |
| `D,<pin>,<enabled>` | Input pull-up bitmask writes |

These commands were retained from prior adapter/reverse evidence. They were not
found on the v0.900 public command page during this audit, so they should remain
documented as legacy source-backed behavior until firmware docs, source, traces,
or hardware validation identify their current v0.900 equivalents.
