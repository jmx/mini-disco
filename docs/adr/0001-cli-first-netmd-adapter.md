# CLI-First NetMD Adapter

Mini Disco starts with a narrow Linux USB CLI that lists the connected NetMD device and the inserted disc contents. The CLI talks to an internal device boundary rather than directly to USB/protocol APIs, with the first adapter built on the Rust `minidisc` crate and `cross_usb`; this keeps the initial slice small while leaving room to replace the protocol library or add a TUI without changing command behavior.
