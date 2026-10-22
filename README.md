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

The bar is a top layer with a 36-pixel exclusive zone. Overview fills the output,
so both surfaces render the same complete `knave_ui::status_bar::StatusBar`
component, including the notification bell, unread count and DND state. Overview
routes the component's shared hit targets for workspace switching and notifications.
Overview also shows a
workspace carousel, search, and a minimized-window shelf for the browsed
workspace. Left/Right, arrows, side previews, dots, and bar chips switch the
active workspace while keeping Overview open. Its preview and bar follow live
workspace changes. Click the center preview or press Enter to return to it. Click
a minimized card to restore it. Type or press Ctrl+K to search installed applications and press Enter to launch the top match, use Tab
to traverse controls, and Escape to hide overview. Super toggles
the session-owned Overview process. Entering the center workspace, restoring a
window, or launching an app hides Overview after the desktop service acknowledges
success. Each reopening starts with an empty search on the active workspace; the service keeps its app catalog and icon
cache. The overview displays
compositor-rendered workspace surfaces inside the cards;
the surfaces are visual only and card controls retain input. Both roles consume
Knave's desktop contract and keep desktop IPC off the Wayland frame thread.

## Build

Building requires GLib/GIO and Polkit development libraries discoverable by
`pkg-config` (`glib-2.0`, `gio-2.0`, `polkit-gobject-1`, `polkit-agent-1`).


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
retained UI toolkit; both hosts render the shared status bar display list. Individually interactive
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

### Notifications

Knave Session starts `knave-shell notifications` alongside the bar. The bar bell
has a top-right unread badge, capped at 99+, and a DND marker. Click it for
scrollable, application-grouped history, actions, per-app banner muting, Do not
disturb and clear/read controls. Escape or an outside click closes the center.
Passive banners never request keyboard focus and leave the desktop outside their
cards clickable. History is paged in groups of 20 to bound retained UI size.
Long titles, bodies and action labels wrap; lengthy bodies have Show more/less.
Light/dark and high-contrast colors use Knave's appearance settings.

The session service owns persistence and delivery. Expired banners remain in
history; restored history has no stale actions. Transient notifications do not
enter history or the count. If another daemon acquired notification delivery
first, the bell indicates external delivery and shows saved Knave history only.
Disconnected state is explicit. Match this Shell with the Knave revision supplying
[ADR 0012](../knave/docs/architecture/decisions/0012-notifications.md); the session
checks `--supports-notifications-service` before starting the role.

### Application applets

StatusNotifier tray icons appear immediately left of the notification bell in
both the bar and Overview. Left-click activates the app (or opens a menu-only
applet), right-click opens its menu, middle-click sends its secondary action,
and scrolling sends vertical scroll events. Exported dbusmenu menus support
submenus, disabled actions and check/radio states; Escape, Close or an outside
click dismisses the menu. Attention icons have a marker; passive items are hidden.

The shell hosts both KDE and freedesktop StatusNotifier interfaces on the
session bus and uses an existing KDE watcher when one is present. Registrations
are deduplicated and removed when their bus owner exits. One event-driven worker
per bar host owns a 16-command queue and at most 32 items. Icons that fit the output
are shown; narrow outputs retain space for the workspace label and bell. Legacy
XEmbed tray icons are not supported.

### Quick settings

Click Settings beside the notification bell in the bar or Overview for a native,
full-height panel. It combines Ethernet/Wi-Fi, Bluetooth, output/microphone mute
and volume, brightness, power profiles, configurable battery charge limit, and
manual/scheduled Night Light. Audio devices and application volumes open in
pavucontrol; additional network profiles open in nm-connection-editor. Do not
disturb remains in Notifications. Missing hardware/services are shown explicitly.

The session starts the `quick-settings` role and its Knave-owned service. It needs
NetworkManager/nmcli, wpctl (WirePlumber), pavucontrol, brightnessctl,
powerprofilesctl and bluetoothctl for the corresponding controls. Battery charge
thresholds need kernel/firmware support and, when root-owned, pkexec. Knave starts its native `polkit-agent` shell role for authentication and releases the panel's
input while the prompt is active. Power lives in the main header and confirms in
a modal popup; subpages contain no Power footer or Refresh button. Brightness
supports 1–100%. There is no dependency on hyprsunset. Deploy matching
Knave and Villain revisions; see Knave ADR 0013.

### Authentication

Knave Session supervises `knave-shell polkit-agent` in direct desktop sessions. It registers
with the system Polkit authority for the current logind session, and shows an
exclusive overlay only during authentication. Select an offered administrator,
enter the requested response, then press Enter or Authenticate. Escape or Cancel
cancels the request. Hidden responses are masked and excluded from clipboard
and IME surrounding text; Ctrl+A replaces the response and Backspace deletes.
No other desktop's agent is required. Registration failures are logged; stop
another agent before restarting Knave if one already owns the session. The
agent needs a system bus, Polkit with its authentication helper, and a logind
session. Winit sessions and Auto sessions with inherited display variables skip
the agent to protect the enclosing desktop. For a direct launch with inherited
display variables, select `backend = "tty"` explicitly.
