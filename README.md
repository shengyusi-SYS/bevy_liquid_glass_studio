# Bevy Liquid Glass

Liquid glass for Bevy UI with a shared live backdrop, refraction, blur, and fused shapes.

![Interactive liquid glass preview](docs/images/preview.png)

## Try it

Use Rust 1.97.1 or newer. On Ubuntu or Debian, install `libasound2-dev`,
`libwayland-dev`, and `libxkbcommon-dev`. Run the examples from the repository root.

```sh
cargo run --locked --example ui_panel
cargo run --locked --example minimal
```

The panel lets you import a background, tune the effect, and copy Rust settings.
On WSLg, use `./examples/preview.sh --locked` for D3D12 through OpenGL and X11.

## Use it in an app

Use the pinned [Bevy 0.20.1 fork](https://github.com/shengyusi-SYS/bevy/tree/b4b5717b05ee3c8580826ce2514cf47642a8e422).
It provides `UiPassSegments` for capturing the backdrop between UI draws.

```toml
[dependencies]
bevy = { version = "=0.20.1", default-features = false, features = ["bevy_winit", "default_font", "x11", "wayland"] }
bevy_liquid_glass = { git = "https://github.com/shengyusi-SYS/bevy_liquid_glass_studio.git", tag = "v0.1.0" }
```

Copy the complete `[patch.crates-io]` section from [Cargo.toml](Cargo.toml)
into your workspace root so the app and library use the same Bevy source.

1. Add `LiquidGlassPlugin` after Bevy's `DefaultPlugins`.
2. Attach `GlassBackdrop` to the UI camera.
3. Add `GlassNode` to a UI node. For fused shapes, put `GlassMember` nodes
   under a `GlassGroup`.

See the [minimal example](examples/minimal.rs) for a runnable setup in this repository.

## Rendering behavior

- Each camera captures the scene and preceding UI before its first visible
  glass surface. All glass on that camera shares the capture, and later UI
  stays sharp. Set `GlassBackdrop.source` to use a full-target image instead.
- Backgrounds refresh each frame by default. For static content, use
  `GlassBackdropRefresh::OnDemand` and mark the backdrop changed after
  changing the scene or UI, or writing to the source image on the GPU.
- The library targets native SDR UI. Groups support axis-aligned movement and
  scaling and inherit clipping from their container or its ancestors.

See the [architecture notes](docs/architecture.md) for blur resolution, group
limits, and coordinate units.

## Preview debugging

Run `cargo run --locked --example ui_panel --features ui-debug` to enable the
local BRP bridge at `127.0.0.1:15704`. Run
`python3 examples/ui_debug.py --help` for inspection and screenshot commands.

## License

The library's original code is MIT licensed. Optics are adapted from
[Liquid Glass Studio](https://github.com/iyinchao/liquid-glass-studio) (MIT),
and UI capture is adapted from
[Beverly](https://github.com/AllTheCodeThatVictorWroteInBali/beverly) (Apache-2.0).
See [LICENSE](LICENSE), [NOTICE](NOTICE), and
[THIRD_PARTY_LICENSES](THIRD_PARTY_LICENSES).
