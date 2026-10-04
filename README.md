# amd-intertouch

Synaptics InterTouch (RMI4 over SMBus) for touchpads on AMD-based ThinkPads.

On these systems the Linux kernel falls back to the PS/2 protocol for the touchpad. `amd-intertouch` provides patched builds of `i2c-piix4`, `rmi_smbus` and `psmouse`, installed through DKMS, that switch the touchpad to RMI4 over SMBus, the transport used by the Windows driver.

## Background

The kernel's RMI4 SMBus driver requires SMBus Host Notify, which lets the touchpad signal that new data is available. `i2c-piix4`, the driver for the AMD FCH SMBus controller, does not implement it, so `psmouse` keeps the touchpad on PS/2.

The FCH exposes an ASF target block on its auxiliary SMBus port, described by the ACPI `SMB0001` device. The patched `i2c-piix4` uses it to receive Host Notify, which allows `rmi_smbus` to bind to the touchpad.

| | PS/2 | RMI4 over SMBus |
|---|---|---|
| Report rate | 80 Hz, 40 Hz with two fingers | full rate with any number of fingers |
| Contacts per report | one, alternating | all |
| Per-contact pressure and size | limited | yes |
| Interrupt | shared PS/2 line | dedicated, through Host Notify |

The TrackPoint remains functional through the touchpad's PS/2 pass-through.

## Supported hardware

| Touchpad ID | System | Status |
|---|---|---|
| `LEN2064` | ThinkPad T14 / P14s Gen 1 AMD | supported, NoSleep enabled by default |
| `LEN2073` | ThinkPad T14 / P14s Gen 2 AMD | supported |
| `LEN0411` | ThinkPad L14 Gen 1 AMD | expected to work, unconfirmed |

Requirements:

- AMD FCH SMBus controller (`1022:790b`)
- ACPI `SMB0001` device
- Synaptics touchpad (Elan touchpads are not supported)
- Linux 7.2 series; other series are flagged by `amd-intertouch check`

## Installation

### Arch Linux

Install the headers for your kernel (`linux-headers`, `linux-lts-headers`, `linux-zen-headers` or `linux-hardened-headers`), then build the package:

```sh
sudo pacman -S --needed base-devel git
git clone https://github.com/meetox80/amd-intertouch
cd amd-intertouch/packaging/arch
makepkg -si
```

Reboot, then verify with `amd-intertouch status`. DKMS rebuilds the modules on kernel updates. To update, run `git pull` and `makepkg -si` again in the same directory. The package conflicts with `thinkpad-t14-amd-touchpad-dkms`.

### Other distributions

Requires `git`, `dkms`, kernel headers, `make`, `gcc`, `patch` and a Rust toolchain.

```sh
git clone https://github.com/meetox80/amd-intertouch
cd amd-intertouch
cargo build --release
./target/release/amd-intertouch check
sudo ./target/release/amd-intertouch try
sudo ./target/release/amd-intertouch install
```

`install` copies the binary to `/usr/local/bin`. The kernel sources and patches are embedded in the binary, so it can be used on another machine without the repository.

## Usage

| Command | Description |
|---|---|
| `check` | Report hardware and build compatibility |
| `status` | Show the touchpad transport, Host Notify state, loaded module builds, DKMS state, NoSleep and TrackPoint settings |
| `try` | Build the modules into `/var/cache/amd-intertouch` and load them until the next reboot |
| `restore` | Reload the installed modules |
| `install [--nosleep MODE]` | Install the modules through DKMS for every kernel with headers |
| `uninstall` | Remove the DKMS modules, configuration and binary |
| `nosleep auto\|on\|off [--now]` | Configure whether the touchpad sensor stays awake between touches |
| `trackpoint [--rate N] [--sensitivity N] [--remove]` | Persist TrackPoint rate and sensitivity through a udev rule |

Commands that modify the system require root and accept `--dry-run`, which prints every action without performing it.

Exit codes for `check`: `0` supported, `2` compatible hardware on an unconfirmed model, `1` not compatible. Usage errors exit with `64`.

Recommended order on a new machine: `check`, then `try`, then `install`.

## Configuration

### NoSleep

Some touchpads report a corrupted first frame after waking from doze, which libinput interprets as a pinch gesture. NoSleep keeps the sensor awake between touches, at a cost of a few milliwatts. It is enabled by default for `LEN2064`.

```sh
sudo amd-intertouch nosleep on --now
```

The setting is stored as `psmouse.synaptics_rmi_nosleep`: `-1` uses the built-in list, `1` forces it on, `0` forces it off.

### TrackPoint

```sh
sudo amd-intertouch trackpoint --rate 200 --sensitivity 200
```

Writes a udev rule applied on every boot and resume, and sets the values immediately. Valid rates are 10, 20, 40, 60, 80, 100 and 200. Sensitivity ranges from 1 to 255, with a hardware default of 128.

## Troubleshooting

- **The touchpad stays on PS/2 after `try`:** run `sudo amd-intertouch restore` and open an issue with the output of `amd-intertouch status` and `journalctl -k -b | grep -iE 'piix4|asf|rmi4|synaptics'`.
- **Modules fail to load with Secure Boot or kernel lockdown enabled:** unsigned DKMS modules are rejected. Sign them with an enrolled MOK or disable Secure Boot.
- **`Failed to write to F03 TX register (-6)` in the kernel log:** a single occurrence during initialization is expected and harmless.

## Patches

| Patch | Module | Change |
|---|---|---|
| `0001` | `i2c-piix4` | Host Notify via the ASF block on `SMB0001` platforms |
| `0002` | `rmi_smbus` | 300 ms delay before reconfiguring the pad on resume |
| `0003` | `psmouse` | Doze interval 6 on the Lenovo pads listed by the Windows driver |
| `0004` | `psmouse` | InterTouch for `LEN2073` |
| `0005` | `psmouse` | InterTouch and NoSleep for `LEN2064`, `synaptics_rmi_nosleep` parameter |
| `0101`, `0102` | `psmouse` | Backports from mainline |

The pristine sources are taken from Linux 7.2.3 and patched at build time.

## Development

```sh
cargo test
cargo test -- --ignored
```

The ignored test applies the patches and builds the modules against the running kernel's headers.

## License

GPL-2.0-only.
