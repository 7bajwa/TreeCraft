# TreeCraft Modeler

A procedural tree modeler inspired by SpeedTree — build trees from a graph of generators
(trunk, branches, leaves, fronds, decorations), shape them with spine/skin/displacement settings,
and preview them in a built-in software renderer.

> Status: early / in active development. See [`FEEDBACK.md`](FEEDBACK.md) for the roadmap and changelog.

## Download (Windows)

Grab the latest `TreeCraft-vX.Y.Z-windows.zip` from the
[Releases](https://github.com/7bajwa/TreeCraft/releases) page, unzip, and run `treecraft.exe`.

## Features

- Node graph of generators: Tree → Trunk / Branch / Leaf / Frond, plus decoration kinds
  (Decal, Knot, Cap, Cut, Flake, Zone, Photogrammetry).
- Generation modes (Interval, Absolute, Phyllotaxy, Bifurcation, Flood, …) from the SpeedTree v10 model.
- Properties panel with Blender-style fields, variance, and a curve editor.
- Materials with texture-map slots, cutout/leaf-card editor, wind, season, LOD.
- Export to OBJ (FBX / glB in progress).
- 2K-ready UI, dark theme, Welcome screen, persistent preferences.

## Build from source

Requires a [Rust toolchain](https://rustup.rs/).

```bash
cargo run --manifest-path app/Cargo.toml            # debug run
cargo build --release --manifest-path app/Cargo.toml # release build
```

The release binary lands at `app/target/release/treecraft.exe`.

### Package a Windows release

```powershell
powershell -ExecutionPolicy Bypass -File scripts/build-release.ps1
```

This builds in release mode and writes `releases/TreeCraft-v<version>-windows.zip`.

## License

All rights reserved (for now).
