# Knave Shell

Knave Shell is the Rust/wgpu user-facing shell for the independent Knave
Desktop Environment. It owns presentation and interaction surfaces while
Villain owns compositor policy, window state, focus, and input dispatch.
Winit is not a host-desktop integration: it is only a nested development
backend. The direct Wayland layer-shell path is the runtime target.

## Runtime

The optional development harness in `../knave/docs/desktop-testing.md` uses
`KNAVE_HARNESS_READY_DIR` for one-shot readiness markers. Configured shell services
signal after Wayland/renderer initialization and receipt of desktop state; the bar
also waits for a compositor frame callback. This private test handshake adds no
watcher and is inactive during normal sessions. Build this Shell revision before
running the matching harness; older Shell builds fail the harness readiness timeout.
Marker I/O failures propagate as typed runtime errors and a nonzero Shell exit.

    knave-shell bar
    knave-shell --supports-overview-service

The bar is a top layer with a 36-pixel exclusive zone. Overview fills the output
with a workspace carousel, search, and a minimized-window shelf for the browsed
workspace. Left/Right browse workspaces; click the center preview or press Enter
to activate it. Click a minimized card to restore it. Type or press Ctrl+K to
search installed applications and press Enter to launch the top match, use Tab
to traverse controls, and Escape to hide overview. Super toggles
the session-owned Overview process. Activating a workspace, window, or app hides
it after the desktop service acknowledges success. Each reopening starts with an
empty search on the active workspace; the service keeps its app catalog and icon
cache. The overview displays
compositor-rendered workspace surfaces inside the cards;
the surfaces are visual only and card controls retain input. Both roles consume
Knave's desktop contract and keep desktop IPC off the Wayland frame thread.

## Build

    cargo fmt --all -- --check
    cargo test --workspace
    cargo clippy --workspace --all-targets -- -D warnings
    cargo build --workspace --release --locked

## Desktop contract

The shell reads desktop state from Knave's JSON-lines API through
knave-desktop-api:

    $XDG_RUNTIME_DIR/knave/desktop-$WAYLAND_DISPLAY.sock

KNAVE_SOCKET overrides the derived path for isolated tests. The client uses one
dedicated subscription worker and a one-entry snapshot mailbox. Villain
pushes changed state; a healthy idle connection makes no periodic requests.
Only failed connections use reconnect backoff, capped at five seconds. It never
blocks the Wayland frame callback on desktop IPC. The Wayland loop blocks when
idle and redraws only for changed state, input, or preview completion; it does
not submit an unchanged frame continuously.

Input actions use a separate one-entry bounded queue and one worker. A full
queue drops an action with an explicit diagnostic instead of creating threads.

The retained overview uses one bounded geometry worker to send up to three
workspace pane rectangles. Transient failures retry with capped backoff, and
new geometry replaces an older pending request. Villain composes existing
window textures during normal output frames. The shell does not request,
decode, or upload PNGs for this overview. A legacy non-application overview host retains its preview worker.

The shell does not own persistent settings. It receives the compositor's
workspace/window state from Villain through Knave's public contract and sends
user actions back through that same contract.

## Install

    scripts/install.sh --user
    scripts/install.sh --system
    scripts/install.sh --prefix "$PWD/stage"

--user installs to ~/.local/bin and --system installs to /usr/local/bin,
requesting sudo only for the target prefix. --prefix is explicit and useful for
packaging. The installer also writes the README below the selected prefix.

## Implementation status

The supported runtime is Rust/wgpu over Wayland layer-shell. Overview uses the
retained UI toolkit; the bar retains its existing scene projection. Individually interactive
window previews, fullscreen-hidden grouping, application launching/icons,
fractional scaling and multiple-output hosting remain future work.

## Workspace

- knave-ui: renderer-independent scene and interaction primitives;
- knave-renderer: render-list, bounded bitmap-text/image painter, and wgpu boundary;
  and
- knave-wayland: direct layer-shell client and bounded desktop-state bridge.

### Native portal controls

`knave-shell portal-picker` uses the existing retained UI toolkit and wgpu renderer
for source previews, explicit consent, cancellation and Stop sharing. The backend
launches it with the bounded JSON contract in `knave-portal-api`; it is
not a user-facing general CLI. No source is selected initially. For a single-screen
share, clicking a card labeled “Share this screen” selects and approves that screen
in one action. Workspace sets and screenshots retain selection plus confirmation. Screen, Window
and Workspaces tabs show choices permitted by the requesting application. Workspace
sets can be noncontiguous; the compact control says Frozen on excluded workspaces.
Active sharing uses a compact 350×36 control at the top-right edge of the bar,
with an app label and one-click Stop sharing. It never requests keyboard focus
and disappears when the portal session ends; it adds no watcher or timer.
Deploy matching Villain, Shell and portal binaries; see Knave ADR 0007.

Static wallpaper is configured through Knave's `[shell.wallpaper]` settings:
`image` is an absolute PNG path and `fit` is `cover`, `contain`, or `stretch`.
Knave Session starts the background role automatically; restart the session to
apply changes. Remove `image` for the black compositor background.
