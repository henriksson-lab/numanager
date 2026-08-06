# WOSM MCU Command Surface

Command surface for `numanager_drivers::wosm`, taken from the project-published
WOSM MCU command reference.

## Evidence Identity

| Field | Value |
| --- | --- |
| Evidence class | Project-published command reference |
| Firmware code base | `v0.900` |
| URL | <https://wosm.net/mcu/commands.php> |
| Hardware validation | Not recorded in this repository |

## Transport

The controller accepts one plain-text command language, reachable from WML,
scripts over Telnet, HTTP arguments, Ajax, a Telnet command line, SPI A, and
firmware-local calls.

| Parameter | Value |
| --- | --- |
| User Telnet port | `23` (configurable for user-Telnet style sessions) |
| Driver/controller-PC Telnet port | `1023` (driver default) |
| Command style | Plain text command plus arguments |
| Prompt completion | `W>` |
| Line ending | CRLF |

## Documented Runtime Mappings

| Runtime operation | WOSM command |
| --- | --- |
| Digital switch/shutter output | `dig_out <val32> <mask32>` |
| Aggregate digital input read | `dig_in` |
| Digital line mode setup | `dig_mode <line> <mode>` |
| Light analog output write | `dac_dest p<s\|t\|u\|v> <0..65535>` |
| Stage X/Y/Z move | `stg_out_x <newpos>`, `stg_out_y <newpos>`, `stg_out_z <newpos>` |
| Stage digital readback family | `stg_val_*`, `stg_dest_*`, `stg_min_*`, `stg_max_*` |
| DAC output readback | `dac_val`, `dac_dest`, `dac_out`, `dac_out_conf` |
| Accurate digital pulses | `dig_hilo`, `dig_lohi` |
| Macro timing | `pause`, `loop`, `dig_wait`, `fast`, `wml_run`, `wml_stop` |

`dac_out` is read-only on v0.900, so writes must use `dac_dest`. The driver maps
public light percentages to unsigned 16-bit DAC destination counts. Stage moves
use the `stg_out_*` abstraction rather than direct DAC writes so board
configuration can choose motor, DAC, or PWM-backed axes.

## Legacy Commands

| Legacy command | Current use |
| --- | --- |
| `P,<index>,<value>`, `N,<count>`, `R`, `E` | Switch-state timing sequence load/run/end |
| `A,<channel>` | Raw analog input readback |
| `D,<pin>,<enabled>` | Input pull-up bitmask writes |

These were carried over from earlier evidence and are **not** on the v0.900
command reference. Treat them as unverified legacy behaviour until firmware
docs, traces, or hardware validation identify their v0.900 equivalents.
