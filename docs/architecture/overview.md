# Workspace overview

`knave-shell::overview::Overview` owns the retained UI and its input state. It
runs as an exclusive full-output layer surface. Villain owns workspace/window
state and composes application surfaces into at most three overview rectangles.
The shell does not configure, resize, focus, or deliver input to those preview
surfaces. Its search, card, and minimized-window controls still dispatch normal
desktop actions and wait for acknowledgement before closing.

The shell computes one selected card and up to two neighboring cards from the
current logical output size. It sends their interior rectangles and workspace
IDs using desktop API 1.3 `set_overview_panes`. The selected pane leaves room
for the minimized shelf, which remains shell-drawn. Pane
contents preserve the output aspect ratio and are clipped to the supplied
rectangle. Browsing and resizing replace the entire set; search clears it.
Villain also clears it when the overview layer surface unmaps or is destroyed.
Search lists installed applications, not windows. `knave-apps` reads XDG
`.desktop` entries once, on the first query, with bounded file counts and sizes;
entries with `NoDisplay`, `Hidden`, `Terminal=true`, or unmet `TryExec`/`OnlyShowIn`
are skipped. Launching sends the existing `Spawn { argv }` desktop command, so
Villain starts the process with the session environment and the overview closes
only after it acknowledges. No new desktop API is involved.
Rows show each application's `Icon`, resolved by `knave-icons` with the icon theme
spec's size rule. Knave has no icon-theme setting and the shell reads no other
desktop's settings, so lookup tries `hicolor`, then every other installed theme by
name, then `pixmaps`. Icons are rasterized at 64px (SVG through resvg without text
or system fonts, PNG downscaled if oversized), at most four per frame for visible
rows only, and kept in a 128-entry cache.
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

This needs Knave desktop API 1.3 and a Villain build that supports live panes.
Deploy Villain before the new shell. Older 1.2 clients work with new Villain;
a new shell on an older compositor keeps its API 1.2 subscription and reports
the unavailable live preview command.
Rollback the shell first. No persistent configuration changes. Direct TTY,
nested GPU composition, focus restoration, and installed binaries require
live smoke verification; a Rust build alone cannot establish them.
