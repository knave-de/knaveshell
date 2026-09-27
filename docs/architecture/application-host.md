# General shell application host

## Decision and ownership

`knave-wayland::run_application` hosts an `Application` using the existing
Wayland/WGPU runtime. The application owns scene state and typed UI actions;
the host owns connection, layer surface, seat resources, GPU lifetime and frame
callbacks. `SurfaceOptions` selects a full-output or centered logical size,
background/bottom/top/overlay layer and none/on-demand/exclusive keyboard mode.
The surface reserves no workspace area. Invalid dimensions and namespaces are
rejected before connecting.

Input is delivered in protocol order, pointer batches coalesce before drawing,
and one outstanding frame callback limits redraw scheduling. Idle applications
have no redraw timer or desktop IPC workers. Keyboard repeat uses the seat's
repeat settings and an owned event-loop timer. Integer output scale updates the
buffer and display transforms while scene coordinates stay logical.

The first available seat owns input. Capability/seat removal cancels gestures,
releases proxies and cancels clipboard transfers. The host currently has one
surface on the compositor-selected output. Per-output hosts, fractional-scale
protocol support, native popup surfaces and concurrent seats need a later
host extension; they are not simulated by the scene toolkit.

## Pointer cursors

The host owns a themed pointer and establishes its cursor on every pointer
entry. `Application::cursor` defaults to a visible arrow; scene applications
return `Scene::cursor()`. Standard shape requests use the compositor's cursor
shape protocol when present. Otherwise SCTK loads cursor images from the
system Xcursor theme, including its scale and hotspot. Missing shapes report an
error and fall back to the theme's arrow. The fallback uses the first theme
frame; animation of client-supplied cursor images is not implemented.

Cursor updates are coalesced until the shape, entry serial or cursor-surface
scale changes. They add no timer, thread or continuous redraw. Pointer loss
drops the pointer, shape device, cursor surface and associated theme resources.
Cursor-surface scale events are kept separate from application buffer scaling.
The existing bar/overview host also establishes arrow/hand cursors.

## Clipboard and text input

Applications return `HostRequest::Copy`/`Paste` and expose the active
`TextInputState`. Clipboard transfer is optional when the compositor provides a
data-device manager. Pipes are nonblocking, capped at four transfers and
16 KiB text, with a two-second timeout and cancellation on seat loss/shutdown.
Late paste responses carry a target ID and are rejected after focus changes.
Failures reach `Application::host_error` and stderr. After this callback the
host checks `should_close` immediately; otherwise it marks the scene dirty and
coalesces a redraw through the existing one-slot wake channel. Asynchronous
failures therefore update error UI without waiting for unrelated input.

Optional text-input-v3 publishes bounded UTF-8 surrounding text (4,000 bytes)
and the logical cursor rectangle. IME updates apply as one batch on `done`;
serial checking rejects batches for an old field. Keyboard text remains
available without the protocol. Preedit cursor/selection attributes and a real
input-method session still need dedicated verification.

## Compatibility and rollout

This is an additive workspace-internal host API at unreleased version 0.1.0;
`WaylandError::InvalidOptions` and `WaylandError::Shm` are additional exhaustive
enum variants. The cursor fallback binds the core `wl_shm` global.
Consumers of the new host rebuild with the same source revision. Existing
`run(ShellRole)` entry points, role namespaces, layer contracts and desktop IPC
payloads remain compatible. No settings or persistent state are migrated.
Villain retains window/workspace/focus policy; this change modifies no
compositor code. Revert the host changes to remove the new entry point while
retaining the previous shell role behavior.

## Verification and remaining coverage

Workspace tests, Clippy and release builds cover all consuming crates. GPU
readback is separate from live Wayland smoke coverage. A sized generic surface
was launched on the development Hyprland session and stayed alive; over a
five-second idle sample it held 3 threads, 67 FDs and about 212 MiB RSS with no
CPU-tick or context-switch increase. This is a short baseline, not a long-run
leak test. Clipboard interoperability, real IME composition, output hotplug,
installed binaries and direct-TTY behavior are not established by that sample.

Cursor regressions cover entry/leave, shape changes, capture, overrides, disabled
controls and 10,000 repeated motions without duplicate requests. A live
Hyprland trace confirmed pointer entry followed by native arrow/hand requests.
The cursor-image fallback and scaled-output appearance have not been live
verified. The brief run settled at three threads and 67 FDs, matching the prior
host baseline; this does not establish long-run resource behavior.
