# Knave Shell

Knave Shell is the Rust/wgpu user-facing shell for the independent Knave
Desktop Environment. It owns presentation and interaction surfaces while
Villain owns compositor policy, window state, focus, and input dispatch.
Winit is not a host-desktop integration: it is only a nested development
backend. The direct Wayland layer-shell path is the runtime target.

## Runtime

    knave-shell bar
    knave-shell overview

The bar is a top layer with a 36-pixel exclusive zone. Overview fills the output
with a workspace carousel, search, and a minimized-window shelf for the browsed
workspace. Left/Right and the arrows/dots browse without changing the desktop.
Click a workspace preview to enter it, or click a visible window in that preview
to enter its workspace focused on that window. Click a minimized card to restore
and focus it. Type or press Ctrl+K to search installed applications and press
Enter to launch the top match; the search clear button removes the query.
Use Tab to traverse controls. Escape or a background click closes the overview;
Super toggles it from Villain. Selection closes only after the desktop service
acknowledges success. The overview displays compositor-rendered workspace
surfaces inside the cards; the shell retains input and Villain resolves preview
clicks against its rendered layout. Both roles consume Knave's versioned desktop
contract and keep desktop IPC off the Wayland frame thread.

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
dedicated API 1.2 subscription worker and a one-entry snapshot mailbox. Villain
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
retained UI toolkit; the bar retains its existing scene projection. Fullscreen-hidden
grouping, fractional scaling and multiple-output hosting remain future work.

## Workspace

- knave-ui: renderer-independent scene and interaction primitives;
- knave-renderer: render-list, bounded bitmap-text/image painter, and wgpu boundary;
  and
- knave-wayland: direct layer-shell client and bounded desktop-state bridge.
