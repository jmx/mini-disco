# Mini Disco CLI

Mini Disco currently exposes a narrow Linux USB NetMD slice.

## Commands

```sh
cargo run -- list
cargo run -- list --json
cargo run -- upload song.wav --title "Track Title"
cargo run -- upload song.wav --format sp --title "Track Title"
cargo run -- upload song.mp3 --format lp2 --title "Track Title"
cargo run -- upload song.mp3 --format lp105 --title "Track Title"
cargo run -- upload song.flac --format lp4 --title "Track Title"
cargo run -- upload-raw track.raw --format sp --title "Track Title"
cargo run -- convert song.mp3 pinball-sp.raw
cargo run -- doctor
```

`list` opens exactly one supported NetMD device, reads the inserted disc, and prints the device name, disc title, track count, capacity, groups, and tracks. If more than one supported NetMD device is attached, unplug all but one device for this first iteration.

`upload` converts any source audio file your installed `ffmpeg` can decode, including MP3, FLAC, WAV, AAC, and Ogg Vorbis, then writes it to the inserted disc. Converted uploads default to SP. SP uses `ffmpeg` to create 44.1 kHz stereo big-endian PCM for the normal NetMD PCM transfer path. LP2, LP105, and LP4 conversion require an `atracdenc` executable in `PATH`; Mini Disco uses `ffmpeg` to create a 44.1 kHz stereo 16-bit WAV, runs `atracdenc`, strips the 96-byte OMA header, checks remaining disc capacity before transfer, and prints the refreshed disc contents after a successful upload.

Importing existing ATRAC1/AEA SP files still needs Web MiniDisc's factory/exploit path and is not implemented yet. The current SP upload path is converted PCM.

`upload-raw` writes prepared raw audio bytes to the inserted disc. `sp` expects big-endian 16-bit stereo PCM. `lp2`, `lp105`, and `lp4` expect headerless ATRAC3 frames. This command does not inspect or convert source audio yet.

`convert` writes those prepared raw bytes without opening a NetMD device. Use it to inspect the conversion step or to test the same output through `upload-raw`.

To create a raw SP file manually:

```sh
ffmpeg -i song.wav -vn -ac 2 -ar 44100 -acodec pcm_s16be -f s16be track.raw
```

`doctor` prints Linux USB permission guidance. The udev rule source of truth is `webminidisc/extra/70-netmd.rules`; install those rules as `/etc/udev/rules.d/70-netmd.rules`, reload udev, and reconnect the NetMD device.

## Scope

This iteration only supports listing, SP/LP2/LP105/LP4 file conversion through external tools, and prepared raw uploads. Rename, delete, playback, factory ATRAC1 SP import, TUI, and cross-platform support are intentionally deferred behind the internal device boundary.
