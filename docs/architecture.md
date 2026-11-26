# Architecture

Install `LiquidGlassPlugin`, configure a camera with `GlassBackdrop`, and attach
`GlassNode` to ordinary UI nodes. Add `GlassGroup` to a UI container and
`GlassMember` to the shapes that should fuse. `GlassShape` describes geometry;
`GlassStyle` describes optics; `GlassBackdrop` describes shared background processing. See
[GLOSSARY.md](../GLOSSARY.md) for the domain vocabulary.

| Module | Responsibility |
|---|---|
| `style` | Shape, optical and shadow settings with each parameter's own units |
| `material` + glass shader | Single/union SDF coverage, refraction, dispersion, Fresnel, glare, tint and union shadow |
| `material_cache` | Intern immutable independent-node materials by their encoded uniform and image identities, without retaining unused assets |
| `ui` | Connect layout/camera targets to material assets; manage target images |
| `group` | Resolve descendant membership, maintain a union surface and synchronize its bounds/material |
| `render` | Extract camera/node data, request a native UI pass segment and record shared capture/blur work |
| `render::blur` | Cache the two Gaussian passes, uniforms and texture bindings |
| `examples` | Official Feathers controls, image import, pointer interactions and optional BRP tools |

## Frame flow

The native UI phase remains responsible for drawing UI geometry. The library
requests a `UiPassSegments` callback before the first glass node. Bevy preserves
that boundary while preparing batches, renders the prefix with native draw
commands and closes the render pass. The callback copies that result into the
camera's source image and runs the shared blur passes in the same command
encoder. Bevy then draws the remaining UI. Foreground
text and controls therefore remain sharp. The first glass node may be an
individual surface or a group's union surface; group members do not draw a
second glass material. The library does not replace the Core2d/Core3d UI systems
or insert synthetic phase items. Bevy owns viewport, draw ranges, diagnostics
and per-frame segment cleanup; the callback only records glass background work.

Each configured camera owns its backdrop images. A camera without glass nodes
uses normal UI drawing. `GlassBackdrop.source` can instead provide an existing
full-target image, so applications retain control over background generation.

All glass nodes on a target sample one shared capture. Later glass nodes do not
recapture earlier glass or intervening UI. Background UI should precede the
first glass node. This explicit first-version semantic avoids a blur pass for
every surface; additional capture groups can be introduced without changing
how applications describe node styles.

## Fused groups

A `GlassNode` has its own `GlassShape` and `GlassStyle` and stays independent,
even inside a group container. A `GlassMember` supplies only a `GlassShape` and
belongs to its nearest ancestor `GlassGroup`. `GlassGroup.style` supplies optics
and shadow; it contains no ignored geometry settings. `blend_radius` controls
smooth union in logical pixels; zero gives a hard union. A member outside all
groups has no glass surface; replace it with `GlassNode` to make it independent.

On hierarchy/component changes, the `group` module resolves affected membership and creates an
internal `GroupSurface` child behind the container's content. After layout and
backdrop preparation, dirty groups gather their visible members and expand combined bounds
for blending and shadow, and updates the surface's `ComputedNode` and
`UiGlobalTransform` in the same frame. The member nodes retain native layout,
children and input. The group shader shades the union silhouette, including its
shadow, rather than applying a rectangular shadow to the bounds.

Each group retains its member list; structural edits rebalance affected groups
in stable entity order. Geometry, transform, clipping, visibility and style changes
mark only dependent groups dirty. Scratch containers are reused. Texture binding
refreshes are separate from shape/uniform reconstruction, and layout can restore
cached overlay render geometry without visiting its members. Transparent shadows
add no shadow padding; fusion and edge feathering retain the needed coverage.

Independent nodes similarly rebuild effective material data only when their
inputs change. Equal configurations share a material AssetId, so updates resolve
another immutable configuration instead of mutating a material used by other nodes.
The key is produced with Bevy's encase serializer into a fixed stack buffer. The
cache holds AssetIds rather than strong Handles; normal asset lifetime management
reclaims unused configurations.

The group material packs up to `MAX_GLASS_GROUP_NODES` (64) shapes into a fixed
uniform array; no fragment storage buffer is required. Additional descendants
render individually using their ancestor group's style. Groups support axis-aligned movement and scaling,
and inherit clipping from the group container and its ancestors. Independent
clipping inside member branches and rotated members are outside the current
contract. Background UI belongs before the group container; the container
itself should hold the glass controls and their foreground content.

## Resources and color

Target size or blur-resolution changes create new image handles, invalidating
ordinary material bindings. Auto uses half-size blur images at radii of at least
8 physical pixels; FullResolution keeps the reference convolution. Radius zero
uses the raw image directly and allocates no blur intermediates. Raw captures
remain full-size. Each axis scales its kernel radius and texel step to the blur
image dimensions, preserving logical-pixel units at different DPI values.
Blur uniforms and bind groups are cached by texture identity, size and radius.
The Gaussian kernel uses a packed uniform array instead of a
fragment storage buffer; no compute pipeline or second GPU device is needed.

Background refresh policy is independent of node geometry/style. EveryFrame
records capture/blur each frame; OnDemand uses an input revision and records work
only when invalidated. Configuration, source-asset events and target metadata
advance the revision; callers mark GlassBackdrop changed after other scene/UI or
GPU content updates. Render-world texture-view identities invalidate recreated GPU
resources. A shared atomic completion revision is set only after all required GPU
commands were recorded successfully; shader/image preparation delays remain pending.
Cached frames need no UI pass segment, and resume ordinary UI drawing directly.

GlassStatistics exposes work counters and opt-in CPU system timings. Optional
render diagnostics and synchronized benchmarks remain example tooling.

For standard SDR UI targets, source textures are sampled as linear RGB and
blur intermediates are `Rgba16Float`. Gaussian convolution and optical blends
use the original sRGB working values; Lab/LCH conversions decode those values.
The vertical blur intermediate stores sRGB working values, and the final
blurred image returns to linear sampling. The optical shader also returns to
linear RGB before calling the fork's shared `writer_encode` for the resolved
camera compositing mode; glass uniforms do not duplicate that view state.
Geometric lengths such as corner radius, refraction thickness, blur and shadow
use logical pixels; the UI's computed scale is applied when constructing GPU
data. `refraction_distance` is the original model's dimensionless distance
multiplier. Fresnel/glare range controls retain the original falloff model and
are not literal pixel widths.

`GlassLighting::Stable` fixes lighting gain at `sqrt(2)*1000/900` and edge
feathering at 0.9 logical pixels, scaled for DPI. `OriginalCanvas` preserves the
upstream physical-height dependence. Refraction and color math are shared.
The reference preset explicitly selects `OriginalCanvas`.

## Preview parameter flow

The panel and official BRP resource methods update the reflected
`PreviewSettings`. The preview derives `EffectivePreview` once, applies optional
interaction feedback only when enabled, and projects geometry/style onto public
members/groups. Snapshot reports structured parameters and nearest-group
ownership. Clipboard and BRP Rust export call one formatter using effective
values. Debug tools do not overwrite application frame pacing.

## Dependencies and extension points

Bevy provides layout, input, UI material extraction/clipping, resource packing,
pipeline caching and individual-node shadows. Feathers/widgets and `rfd` are example dependencies.
The optional `ui-debug` feature uses official BRP plus a small example bridge;
it is not required by library consumers.

The public node and style types do not know about a specific window, image
picker, pointer-follow behavior or parameter panel. Applications update them
through normal ECS and asset operations. The example exercises that same
interface rather than a separate preview-only renderer.
