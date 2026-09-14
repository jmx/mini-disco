# Mini Disco

Mini Disco is a Linux-first command-line tool for working with USB NetMD MiniDisc devices. It is a small Rust companion to the Web MiniDisc Pro code in this repository: the web app remains the broad, browser-based experience, while Mini Disco focuses on fast local terminal workflows for listing discs, editing titles, uploading audio, and controlling playback.

The Rust tool lives in [`mini-disco/`](mini-disco/). The upstream-style web app lives in [`webminidisc/`](webminidisc/).

## What It Can Do

- Find supported NetMD devices connected over USB.
- List the inserted disc title, groups, tracks, recording details, and remaining time.
- Upload individual audio files after conversion through `ffmpeg`.
- Upload M3U/M3U8 playlists in order, optionally erasing the disc first or creating a group.
- Upload prepared raw SP, LP2, LP105, or LP4 audio bytes.
- Rename discs and tracks.
- Delete tracks or erase the whole disc.
- Eject the disc when the device supports software eject.
- Control playback with play, pause, stop, next, and previous commands.
- Print JSON for device and disc listings when scripts need structured output.

Mini Disco currently targets Linux USB NetMD devices only. It does not try to replace every Web MiniDisc Pro feature yet.

## Requirements

- Linux.
- Rust and Cargo.
- A USB NetMD device and an inserted MiniDisc.
- USB permissions for your user.
- `ffmpeg` in `PATH` for normal audio-file uploads.
- `atracdenc` in `PATH` for LP2, LP105, and LP4 uploads.

For USB permissions, start with:

```sh
cd mini-disco
cargo run -- doctor
```

Mini Disco uses the udev rules from [`webminidisc/extra/70-netmd.rules`](webminidisc/extra/70-netmd.rules) as the source of truth. Install those rules as `/etc/udev/rules.d/70-netmd.rules`, reload udev, and reconnect the NetMD device.

## Running It

From the repository root:

```sh
cd mini-disco
cargo run -- devices
cargo run -- list
```

To build a reusable local binary:

```sh
cd mini-disco
cargo build --release
./target/release/mini-disco devices
```

Or install it from the local source tree:

```sh
cargo install --path mini-disco
mini-disco devices
```

If more than one supported NetMD device is attached, list them first and pass the 1-based device number:

```sh
mini-disco devices
mini-disco --device 2 list
mini-disco --device 2 upload song.flac --title "A Good Track"
```

## Common Workflows

Inspect the current disc:

```sh
mini-disco list
mini-disco list --json
```

Upload a single track in SP mode:

```sh
mini-disco upload song.wav --title "Track Title"
mini-disco upload song.flac
```

Upload in an LP mode:

```sh
mini-disco upload song.mp3 --format lp2 --title "Long Play Track"
mini-disco upload song.flac --format lp105
mini-disco upload song.ogg --format lp4
```

Upload a playlist:

```sh
mini-disco upload-m3u album.m3u --format sp
mini-disco upload-m3u album.m3u --format lp2 --erase-first
mini-disco upload-m3u album.m3u --format lp2 --group
```

Playlist titles come from `#PLAYLIST`, or from `#EXTART` and `#EXTALB`, with the playlist file name as a fallback. Track titles come from preceding `#EXTINF` entries when present, otherwise from the source file name.

Edit disc metadata:

```sh
mini-disco rename-disc "Road Mix"
mini-disco rename-track 3 "New Track Title"
```

Delete content:

```sh
mini-disco delete-track 3
mini-disco erase
```

Control the attached device:

```sh
mini-disco play
mini-disco pause
mini-disco stop
mini-disco next
mini-disco prev
mini-disco eject
```

## Upload Formats

`upload` accepts any file that your installed `ffmpeg` can decode, such as WAV, FLAC, MP3, AAC, or Ogg Vorbis.

- `sp` uses `ffmpeg` to produce 44.1 kHz stereo big-endian PCM and sends it over the normal NetMD PCM upload path.
- `lp2`, `lp105`, and `lp4` use `ffmpeg` to make a temporary WAV, then `atracdenc` to encode ATRAC3. Mini Disco strips the OMA header before transfer.

`upload-raw` is for already prepared bytes:

```sh
mini-disco upload-raw track.raw --format sp --title "Prepared Track"
```

For SP raw files, the input must be big-endian 16-bit stereo PCM. For LP modes, the input must be headerless ATRAC3 frames.

You can test conversion without touching a disc:

```sh
mini-disco convert song.flac song-sp.raw --format sp
mini-disco convert song.flac song-lp2.raw --format lp2
```

## Safety Notes

Commands that change the disc refuse to write when the disc is not writable or the write-protect tab is enabled. Upload commands check available capacity before transfer when the device reports remaining time.

Track numbers are the 1-based numbers printed by `mini-disco list`.

## More CLI Detail

See [`mini-disco/docs/cli.md`](mini-disco/docs/cli.md) for the command reference and implementation notes.

## Todo: Web Version Parity

Mini Disco intentionally started as a narrow Linux CLI. These are the larger Web MiniDisc Pro features and polish items still missing from the Rust tool:

- Terminal UI for browsing discs and choosing actions interactively.
- Track download support, including standard NetMD download for Sony MZ-RH1.
- Factory-mode download support for broader Sony and Aiwa NetMD devices.
- Factory-mode tools such as firmware dumping, RAM dumping, TOC manipulation, and bad-sector workflows.
- Remote NetMD support for devices exposed over the local network.
- Hi-MD support.
- Song recognition and metadata lookup.
- Local library management.
- Import of existing ATRAC1/AEA SP files through the factory/exploit path.
- Richer device capability detection and clearer per-device support reporting.
- Cross-platform support beyond Linux.
- Packaged releases so users do not need a Rust toolchain.

Not all of those belong in the CLI forever, but they are the main gaps to keep visible while Mini Disco grows up from a focused terminal tool into something closer to the web app's coverage.
