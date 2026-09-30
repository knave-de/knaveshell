# Workspace overview

`knave-shell::overview::Overview` owns the retained UI and its input state. Knave
Session starts and supervises exactly one Overview process for the Wayland
session. It remains alive while hidden and maps its exclusive full-output layer
surface only when Villain publishes `overview_visible=true`. Villain owns
workspace/window state and composes application surfaces into at most three
overview rectangles.
The shell does not configure, resize, focus, or deliver input to those preview
surfaces. Its search, card, and minimized-window controls still dispatch normal
desktop actions and wait for acknowledgement before hiding. Escape hides it;
Super toggles the same session-owned instance. Hiding clears the query, focused
control, browsing/pages, pointer gestures, errors, and any launch waiting on the
catalog. Reopening selects the current active workspace. The catalog, icon
cache, loader, desktop snapshot, and in-flight action bookkeeping remain owned
by the service; ordinary visible snapshot updates preserve the current search.
Disconnection also resets the view. This is private Shell behavior with no
protocol or configuration change and no added background activity.

The shell computes one selected card and up to two neighboring cards from the
current logical output size. It sends their interior rectangles and workspace
IDs using desktop API 1.3 `set_overview_panes`. The selected pane leaves room
for the minimized shelf, which remains shell-drawn. Pane
contents preserve the output aspect ratio and are clipped to the supplied
rectangle. Browsing and resizing replace the entire set; search clears it.
Villain also clears it when the overview layer surface unmaps or is destroyed.
Search lists installed applications, not windows. `knave-apps` reads XDG
`.desktop` entries; entries with `NoDisplay`, `Hidden`, `Terminal=true`, or unmet
`TryExec`/`OnlyShowIn` are skipped. Launching sends the existing `Spawn { argv }`
desktop command, so Villain starts the process with the session environment and
the overview hides only after it acknowledges. Visibility uses desktop API 1.4.

Rows show each application's `Icon`, resolved by `knave-icons` with the icon theme
spec's size rule. Knave has no icon-theme setting and the shell reads no other
desktop's settings, so lookup tries `hicolor`, then every other installed theme by
name, then `pixmaps`.

Catalog and icon loading run on one worker owned by the `Overview`, started on the
first typed character. The input and frame paths never read the filesystem. The
job queue holds 32 jobs; icon requests are limited to visible rows and
deduplicated, and finished icons live in a 128-entry cache. The worker wakes the
host through `Application::set_waker`, and `needs_frame` reports delivered
results, so there is no polling. A scan examines at most 16,384 directory entries
and keeps at most 4,096 applications; reaching a limit marks the catalog
incomplete and logs it. Only regular files are opened, so a FIFO cannot block the
worker. The queue closes and the worker is detached when the session ends. Enter
pressed before the catalog arrives launches the top result when it does.
Icons rasterize at 64px (SVG through resvg without text or system fonts, PNG
downscaled if oversized).
Only window textures are composed: there is no screenshot readback, PNG
transport, decoded image cache, or window input remapping in this path.

One bounded pane worker keeps IPC off the Wayland frame callback. It has a
single pending update and wakes the UI after completion, so a full queue
retries the latest layout without spawning more threads. Transient connection
or unavailable errors retry with capped backoff; permanent API errors surface
to the UI. Desktop actions use a separate bounded worker. The snapshot subscription remains event driven;
failed connections use capped retry backoff. The legacy preview query remains
available for older clients, and the non-application overview host still uses
its existing preview worker.

This needs Knave desktop API 1.4 and a Villain build that supports visibility
and live panes. Deploy Knave, then Villain, then the shell. Older 1.2 clients
remain accepted by new Villain; a new session-owned shell requires API 1.4.
Rollback the shell first. No persistent visibility configuration changes. Direct TTY,
nested GPU composition, focus restoration, and installed binaries require
live smoke verification; a Rust build alone cannot establish them.

The additive `--supports-overview-service` capability probe exits successfully
without a Wayland connection for service contract 1. Knave Session bounds it
to two seconds and rejects incompatible Shell binaries. On desktop disconnection,
Overview unmaps locally and keeps its process for reconnection. Pending
activation completions do not queue a hide for an already-hidden surface,
and the host processes requests while hidden.
