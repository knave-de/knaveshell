# Retained scene toolkit

## Context and decision

Shell controls need layout, focus, capture, text editing and shared rendering
across an entire output. `knave-ui::toolkit` now owns those concerns without
Wayland, GPU or desktop-service dependencies. Existing `UiScene` projections
remain available during adoption.

An application composes an `Element` tree with stable `ElementId`s into a
`Scene`. Rows, columns, grids and overlays use logical pixels, padding, gaps,
fixed/automatic/fill lengths and rectangular clipping. The renderer implements
`TextMeasurer` with the same Cosmic Text font system used for painting.
`TextSpan` supports inline color and weight in wrapped paragraphs.

`Scene::event` accepts ordered pointer, keyboard and text events and returns an
optional typed action plus redraw state. Values live in the scene; applications
translate actions into service requests. `insert`, `remove`, `set_widget`,
`set_layout`, `set_style`, `set_enabled` and `set_visible` are explicit mutation
boundaries. Invalid insertions leave the existing scene intact. Replacing a
widget cancels its interaction; unrelated structural changes retain focus,
scroll, capture and popup state by ID.

## Interaction contract

- Hover, pressed, selected, disabled, focused, captured and expanded are
  independent states. Buttons activate on release over the original control.
  Disabled or covering elements block pointer delivery to elements below them.
- Tab/Shift+Tab traverse enabled controls. Arrows navigate spatially, except
  where a control consumes them. Keyboard activation ignores repeat; text
  editing and slider adjustment can repeat. Focused rows scroll into view.
- Sliders support horizontal/vertical tracks, continuous/stepped ranges and
  thumb grab offsets. Pointer motion emits changes; release commits. Escape,
  disabling or removal cancels capture and restores the original value.
- Drags start after four logical pixels, retain capture outside their bounds,
  and report start/move/finish/cancel. They move scene elements; application or
  compositor window dragging is a separate policy.
- Dropdowns and menus use stable option IDs, disabled rows and separators.
  Selection requires release on the pressed row; keyboard navigation skips
  unselectable rows. Menus scroll and stay within the scene viewport.
- Popups render above scene content. Modal popups block background input and
  trap focus; closing restores the opener if still eligible. Outside presses
  dismiss without activating background controls. These are scene popups,
  not separate native Wayland popup surfaces.
- Single-line editing uses grapheme boundaries, selection, clipboard actions
  and preedit/commit events. Glyph-derived carets interpolate within ligatures.
  Full bidirectional selection fidelity and IME preedit cursor attributes are
  not complete. Tooltips are immediate and bounded; there is no idle timer.

Cursor appearance is renderer-independent. `Scene::cursor()` resolves the
current hit target, popup/menu state and pointer capture. Defaults are arrow,
hand, I-beam, horizontal/vertical adjustment, grab/grabbing and not-allowed.
`Element::cursor` and `Scene::set_cursor` provide explicit overrides including
move, diagonal resize, crosshair, wait/progress and intentional hiding. Passive
children inherit the nearest control/override; disabled targets and popup
backdrops do not expose background actions. Layout changes re-evaluate a
stationary pointer without requiring motion.

### Spacing, clipping and wheel parameters

`Layout::with_padding(Insets)` overrides the existing uniform `padding` value;
`with_margin(Insets)` adds non-collapsing space outside the element. Insets have
independent top/right/bottom/left values and `all`/`symmetric` constructors.
Margins participate in row, column and grid allocation but are not hit targets.
Scroll content retains its measured height, including automatic-size grids.

`Layout::clip` selects rectangular child clipping to `Content` (the default),
`Bounds`, or `Visible` overflow. Ancestor and output clips still apply; scroll
containers always clip to their content area. These options do not provide
rounded or arbitrary-path masks. `Style::radii` optionally overrides all four
corners; otherwise a custom background retains its own `ShapePaint::radii`, and
standard controls use the existing uniform `Style::radius`.

Sliders opt in with `slider.with_scroll(Some(SliderScroll {
pixels_per_step: 40.0, inverted: false }))?`; `None` disables wheel adjustment.
Up increases the value by default. Each configured delta unit applies one
slider step, or one percent of the range for continuous sliders. Stepped sliders
accumulate small deltas. Enabled sliders consume scrolling even at endpoints;
disabled wheel handling lets the parent scroll. Changes emit `ValueCommitted`
and repaint without remeasurement. Checkboxes render the Unicode checkmark ✓.

These are workspace-internal source API additions: exhaustive `Layout` and
`Style` literals need the new fields or `..Default::default()`. Existing uniform
padding/radius values and `Slider::new` calls remain supported. No protocol or
configuration migration is involved. Revert the spacing/control change as one
slice to roll back; it has no persisted state.

## Rendering and resource behavior

Measurement, placement and paint invalidation are separate. Hover and slider
motion repaint; scrolling and dragging update placement without text shaping.
Offscreen elements emit no drawing commands. Large collections retain their
nodes; this is clipping, not list virtualization.

Shape/image CPU staging and GPU instance buffers retain capacity across frames.
Text can batch past later primitives only when conservative paint bounds are
disjoint; overlapping paint, shadows and clips retain their visual order.
`SceneStats` and `RendererStats` expose layout/paint/measurement and instance
buffer allocation counts. Full display-list preparation still occurs on each
painted frame; dirty-region rendering is not implemented.

Bounds: 2,048 elements, depth 48, 16 KiB per text value, 1,024 options per menu;
renderer limits remain 8,192 commands, 512 KiB text per frame, 128 text runs,
128 cached layouts/4 MiB estimated layout storage, and 64 MiB cached images.
The renderer reports unsupported transforms or exceeded frame budgets.

## Compatibility, adoption and rollback

The new toolkit is additive to the workspace-internal Rust API.
`DisplayCommand::RichText` adds an exhaustive enum variant: renderer and host
consumers are updated together; external Rust matches require a source update. `Element` also gains an optional
cursor field; constructor users receive automatic defaults, while exhaustive
struct literals must add `cursor: None`.
Workspace crate versions remain 0.1.0, unreleased; the source revision identifies
the compatible combination. No desktop API, configuration schema, compositor
protocol or persisted data changes. Production bar/overview projections are
not migrated by this change. Reverting the toolkit and renderer changes
together restores the prior path without data migration.

## Verification

Interaction regressions cover capture, cancellation, modal restoration,
clipping, menu navigation, disabled hit targets, atomic insertion, Unicode
editing and invalidation. GPU readback covers paint order, real text and stable
instance-buffer allocation after warmup. A 1440x900 offscreen mixed interaction
sample on the development machine measured 48/600 cards at 75/212 microseconds
p95 CPU scene preparation and about 2.14 milliseconds p95 including GPU wait.
Both samples reused one shape and one image instance buffer over 2,100 frames.
Peak RSS was approximately 215/216 MiB, with three threads and 50 FDs. These are
initial baselines, not a measured improvement over the previous implementation
or a compositor presentation-latency guarantee.
