# Mini Disco CLI

Mini Disco currently exposes a narrow Linux USB NetMD slice.

## Commands

```sh
cargo run -- devices
cargo run -- devices --json
cargo run -- list
cargo run -- --device 2 list
cargo run -- list --json
cargo run -- upload song.wav --title "Track Title"
cargo run -- upload song.wav --format sp --title "Track Title"
cargo run -- upload song.mp3 --format lp2 --title "Track Title"
cargo run -- upload song.mp3 --format lp105 --title "Track Title"
cargo run -- upload song.flac --format lp4 --title "Track Title"
cargo run -- upload-m3u album.m3u --format sp
cargo run -- upload-m3u album.m3u --format lp2 --erase-first
cargo run -- upload-m3u album.m3u --format lp2 --group
cargo run -- upload-raw track.raw --format sp --title "Track Title"
cargo run -- convert song.mp3 pinball-sp.raw
cargo run -- rename-disc "Disc Title"
cargo run -- rename-track 3 "Track Title"
cargo run -- delete-track 3
cargo run -- erase
cargo run -- eject
cargo run -- eject --device 2
cargo run -- --device 2 eject
cargo run -- play
cargo run -- pause
cargo run -- stop
cargo run -- next
cargo run -- prev
cargo run -- doctor
```

`devices` lists every supported NetMD USB match without opening the disc. Device numbers are 1-based and are the values accepted by the global `--device NUMBER` option.

`list` opens one supported NetMD device, reads the inserted disc, and prints the device name, disc title, track count, capacity, groups, and tracks. If exactly one supported device is attached, Mini Disco selects it automatically. If multiple supported devices are attached, run `devices`, then pass `--device NUMBER` to `list`, `upload`, `upload-m3u`, `upload-raw`, `rename-disc`, `rename-track`, `delete-track`, `erase`, `eject`, `play`, `pause`, `stop`, `next`, or `prev`.

`upload` converts any source audio file your installed `ffmpeg` can decode, including MP3, FLAC, WAV, AAC, and Ogg Vorbis, then writes it to the inserted disc. Converted uploads default to SP. SP uses `ffmpeg` to create 44.1 kHz stereo big-endian PCM for the normal NetMD PCM transfer path. LP2, LP105, and LP4 conversion require an `atracdenc` executable in `PATH`; Mini Disco uses `ffmpeg` to create a 44.1 kHz stereo 16-bit WAV, runs `atracdenc`, strips the 96-byte OMA header, checks remaining disc capacity before transfer, and prints the refreshed disc contents after a successful upload. Converted uploads print encoder progress before the NetMD upload stage begins.

Importing existing ATRAC1/AEA SP files still needs Web MiniDisc's factory/exploit path and is not implemented yet. The current SP upload path is converted PCM.

`upload-m3u` reads local track paths from an M3U or M3U8 playlist, converts each track, and uploads them in playlist order. Track titles come from preceding `#EXTINF` entries when present, otherwise from the source file name stem. The disc or group title comes from `#PLAYLIST`; if that is absent, Mini Disco combines `#EXTART` and `#EXTALB` as `Artist - Title`; if those are absent, it falls back to the playlist file name stem. By default, `upload-m3u` writes that title to the disc and then uploads the tracks. With `--erase-first`, it erases the inserted disc before writing. With `--group`, it leaves the existing disc title untouched and creates a group with the playlist title around the uploaded tracks.

`upload-raw` writes prepared raw audio bytes to the inserted disc. `sp` expects big-endian 16-bit stereo PCM. `lp2`, `lp105`, and `lp4` expect headerless ATRAC3 frames. This command does not inspect or convert source audio yet.

`convert` writes those prepared raw bytes without opening a NetMD device. Use it to inspect the conversion step or to test the same output through `upload-raw`.

`rename-disc` updates the inserted disc title while preserving group metadata. It refuses to write to a read-only or write-protected disc and prints the refreshed disc contents after a successful rename.

`rename-track` updates one track title by the same 1-based number shown by `list`. It refuses to write to a read-only or write-protected disc, validates that the track exists, and prints the refreshed disc contents after a successful rename.

`delete-track` removes one track by the same 1-based number shown by `list`. It refuses to write to a read-only or write-protected disc, validates that the track exists, and prints the refreshed disc contents after a successful delete.

`erase` removes all tracks and title metadata from the inserted disc. It refuses to write to a read-only or write-protected disc and prints the refreshed disc contents after a successful erase.

`eject` asks the attached NetMD device to eject the inserted disc. Not every supported device implements software eject; unsupported devices may reject the command.

`play`, `pause`, `stop`, `next`, and `prev` control playback on the attached device. These commands do not modify the disc and do not require a writable disc.

To create a raw SP file manually:

```sh
ffmpeg -i song.wav -vn -ac 2 -ar 44100 -acodec pcm_s16be -f s16be track.raw
```

`doctor` prints Linux USB permission guidance. The udev rule source of truth is `webminidisc/extra/70-netmd.rules`; install those rules as `/etc/udev/rules.d/70-netmd.rules`, reload udev, and reconnect the NetMD device.

## Scope

This iteration only supports listing, disc rename, track rename/delete, whole-disc erase, software eject, M3U playlist upload, playback controls, SP/LP2/LP105/LP4 file conversion through external tools, and prepared raw uploads. Factory ATRAC1 SP import, TUI, and cross-platform support are intentionally deferred behind the internal device boundary.
