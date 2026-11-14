# Ethornell browser distribution

Ethornell and the Retrom integration are available under MPL-2.0 as declared
by the upstream project. Upstream baseline:
https://github.com/xmoezzz/ethornell-rs/tree/7cfc2fba8261e59c68095ab1ddc4f1f3534c27d8

The MPEG decoder is mixed-provenance code. In particular, its Rust IDCT and
MPEG tables adapt FFmpeg code under LGPL-2.1-or-later, with the retained
Michael Niedermayer, Fabrice Bellard and Aaron Holtzman provenance notices
in `crates/na_mpeg2_decoder/src/video/{idct,tables,vlctables}.rs`.
FFmpeg n7.1 is a verified comparison reference; the upstream port did not
record the exact original imported revision. This is not a claim that
every MPEG source file was authored by FFmpeg contributors.

The complete engine sources and selected normal/build Cargo dependency sources
are included in `ethornell-source.tar.gz`. RETROM.md describes rebuilding the
combined Wasm, including substituting modified decoder sources. There is no
restriction on reverse engineering this combined work to debug such changes.
Unused WMV and Flash implementations are not linked into this browser build.
The separate default Noto Sans SC font is covered by FONT-OFL.txt.

Keep these notices and all license texts with the runtime assets. Corresponding
source is distributed beside them at immutable fork releases:
https://github.com/retrom-project/ethornell-rs/releases
