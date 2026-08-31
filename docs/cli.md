# Mini Disco CLI

Mini Disco currently exposes a narrow Linux USB NetMD slice.

## Commands

```sh
cargo run -- list
cargo run -- list --json
cargo run -- doctor
```

`list` opens exactly one supported NetMD device, reads the inserted disc, and prints the device name, disc title, track count, capacity, groups, and tracks. If more than one supported NetMD device is attached, unplug all but one device for this first iteration.

`doctor` prints Linux USB permission guidance. The udev rule source of truth is `webminidisc/extra/70-netmd.rules`; install those rules as `/etc/udev/rules.d/70-netmd.rules`, reload udev, and reconnect the NetMD device.

## Scope

This iteration is read-only. Rename, delete, playback, upload, TUI, and cross-platform support are intentionally deferred behind the internal device boundary.
