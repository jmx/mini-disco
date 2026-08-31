# Mini Disco CLI

Mini Disco currently exposes a narrow Linux USB NetMD slice.

## Commands

```sh
cargo run -- list
cargo run -- list --json
cargo run -- upload song.wav --format sp --title "Track Title"
cargo run -- upload-raw track.raw --format sp --title "Track Title"
cargo run -- doctor
```

`list` opens exactly one supported NetMD device, reads the inserted disc, and prints the device name, disc title, track count, capacity, groups, and tracks. If more than one supported NetMD device is attached, unplug all but one device for this first iteration.

`upload` converts a source audio file with `ffmpeg` and writes it to the inserted disc. Only SP conversion is implemented in this iteration. The generated raw stream is 44.1 kHz, stereo, big-endian 16-bit PCM.

`upload-raw` writes prepared raw audio bytes to the inserted disc. `sp` expects big-endian 16-bit stereo PCM. `lp2` and `lp4` expect headerless ATRAC3 frames. This command does not inspect or convert source audio yet.

To create a raw SP file manually:

```sh
ffmpeg -i song.wav -vn -ac 2 -ar 44100 -acodec pcm_s16be -f s16be track.raw
```

`doctor` prints Linux USB permission guidance. The udev rule source of truth is `webminidisc/extra/70-netmd.rules`; install those rules as `/etc/udev/rules.d/70-netmd.rules`, reload udev, and reconnect the NetMD device.

## Scope

This iteration only supports listing, SP file conversion, and prepared raw uploads. Rename, delete, playback, LP2/LP4 conversion, TUI, and cross-platform support are intentionally deferred behind the internal device boundary.
