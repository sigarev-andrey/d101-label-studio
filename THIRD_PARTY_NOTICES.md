# Sources and acknowledgements

NIIMBOT protocol facts and print sequences were checked against:

- NiimBlueLib, https://github.com/MultiMote/niimbluelib, commit `35110a716c960448ee17a9270e4c3a131149866f`.
- D101 hardware profile: https://printers.niim.blue/hardware/niimbot-d101/
- Connection: https://printers.niim.blue/interfacing/connecting/
- Print tasks: https://printers.niim.blue/interfacing/print-tasks/
- Community hardware reports: https://github.com/MultiMote/niimbluelib/issues/1

Protocol constants, version interpretation and packet layouts are implemented in Rust using these references. NiimBlueLib's MIT notice is included below.

Batch throughput changes also use the bounded page quantity field documented in [B1PrintTask](https://github.com/MultiMote/niimbluelib/blob/main/src/print_tasks/B1PrintTask.ts) and [D110MV4PrintTask](https://github.com/MultiMote/niimbluelib/blob/main/src/print_tasks/D110MV4PrintTask.ts). Only consecutive equal rasters are combined; page order and physical completion checks are preserved. BLE pacing is applied once per complete packet, accounting for elapsed writes/acknowledgements; the 10 ms minimum follows the adapter compatibility findings in [NiimBlue issue #5](https://github.com/MultiMote/niimblue/issues/5).

PWG Raster / IPP references (independent Rust implementation, CUPS is not linked):

- https://www.pwg.org/ipp/ippguide.html
- https://ftp.pwg.org/pub/pwg/candidates/cs-ippraster10-20120420-5102.4.pdf
- https://github.com/OpenPrinting/cups/blob/master/cups/raster-stream.c
- https://learn.microsoft.com/powershell/module/printmanagement/add-printer
- https://learn.microsoft.com/en-us/windows/modern-print/windows-protected-print-mode/more-information-on-windows-protected-print-mode-for-enterprises-and-developers

Rust dependencies and versions are recorded in Cargo.lock. Main dependencies include windows-reactor/windows-sys (MIT OR Apache-2.0), btleplug (MIT OR Apache-2.0 OR BSD-3-Clause), tokio/axum (MIT), image (MIT OR Apache-2.0), ab_glyph (Apache-2.0), rfd (MIT), csv/serde/serde_json/anyhow (MIT OR Apache-2.0), futures (MIT OR Apache-2.0), uuid (Apache-2.0 OR MIT).

WinUI 3 is hosted using Microsoft's Windows Reactor: https://github.com/microsoft/windows-rs/tree/master/crates/libs/reactor (pinned crate 0.100.0, preview API). The Windows App Runtime is a separate Microsoft prerequisite and is not redistributed in this archive. Text rasterization uses GDI and the installed Windows fonts.

Segoe UI is loaded from the user's Windows installation, and is not redistributed.

## NiimBlueLib license

MIT License

Copyright (c) 2024 MultiMote

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
