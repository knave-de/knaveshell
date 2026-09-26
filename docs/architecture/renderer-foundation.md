# Renderer foundation

Status: implemented on `feat/renderer-foundation`.

## Context

The original painter mixed GPU setup, command preparation, primitive batching,
image caching, and a hand-authored bitmap font in one module. It batched fills
and text separately from images, so mixed display-list order was not preserved.
That made clipping, effects, text layout, and focused rendering tests difficult
to add without growing the same module further.

## Decision

- `knave-ui` owns logical geometry, affine transforms, paint styles, and the
  ordered renderer-independent display list. Its builder captures transform
  and nested rectangular clip state in each command.
- `knave-renderer` coordinates separate shape, image, and text GPU modules in
  one render pass, visiting commands in their declared order.
- Shapes use signed-distance rounded rectangles with fill, border, opacity,
  spread, offset, and blurred shadows. Images support stretch/contain/cover,
  opacity, rounded clipping, affine transforms, and a byte-budgeted GPU LRU.
- Text uses Cosmic Text for system font discovery, shaping, fallback, wrapping,
  and layout; Glyphon supplies the WGPU glyph atlas and draw path. Font and atlas
  state live with the painter. Shaped layout and text-run caches are bounded.
- `Color` stores sRGB-encoded 8-bit channels. Shape and clear colors are
  converted to linear light, images use sRGB textures, and Glyphon performs
  text-color decoding. Wayland chooses a supported sRGB surface format, and the
  painter rejects non-sRGB targets so output values remain consistent.
- Rectangular clips are intersected in UI space and enforced with GPU scissor
  rectangles. Rotated clips currently use conservative axis-aligned bounds.

## Compatibility and limits

`DisplayCommand` variants/fields and the fallible, sRGB-only
`WgpuPainter::new` changed at the Rust crate boundary; the renderer, Wayland,
and shell workspace consumers were updated together. There is no change to the
Knave desktop API, shell/Wayland protocol, configuration, or compositor
behavior. These renderer crates are workspace-internal implementation APIs;
external Rust consumers would need a source update before adopting this
revision.

Text currently supports translation and positive uniform scaling. Rotation,
shear, and non-uniform text scaling return an explicit frame-preparation error;
shape and image transforms support general affine matrices. Animation scheduling,
backdrop blur, non-rectangular clipping, and persistent cross-frame GPU scene
buffers are outside this foundation slice.

## Rollback and verification

Reverting this branch restores the previous painter and dependency graph; no
configuration or persisted user data is migrated. CPU tests cover scene
projection, ordering, transforms, clips, text transform constraints, image fit,
shadow geometry, and scissor conversion. A headless WGPU readback test covers
mixed primitive order, clipping, rounded shape output, affine transforms,
shadows, and real system-font pixels. The test can dump its small PNG frame by
setting `KNAVE_RENDERER_TEST_FRAME`. The 1200x820 visual gallery test exercises
multiple font sizes, shape effects, image fit modes, clipping, and transforms;
set `KNAVE_RENDERER_GALLERY_FRAME` to save its PNG for review.

The readback test ran through Vulkan on the available Intel integrated GPU. It
does not prove live Wayland, installed-binary, or direct-session behavior.
Idle/normal/stress CPU, memory, and latency measurements are still required
before making performance claims.
