# Workspace overview: first retained UI slice

## Decision and ownership

`knave-shell::overview::Overview` owns the overview controller and composes the
retained toolkit. `knave-shell overview` uses an exclusive, full-output overlay
through the general application host. The bar remains on its existing path.
The existing public desktop wire contract is unchanged; the host supplies
snapshots, workspace preview images, and acknowledged desktop actions.

The UI has a search field, a centered workspace with immediate neighbors,
workspace indicators and a paginated minimized-window shelf. Each minimized
window appears individually in its own workspace. Workspace screenshots retain
their aspect ratio. They are snapshots of compositor-rendered content, not
individual interactive window surfaces. Hidden-by-fullscreen grouping, app
icons/favorites, wallpaper blur, animations and live per-window previews are
later slices. Search currently indexes window titles, app IDs and workspace
labels; it does not launch installed applications.

## Interaction and state

Browsing a neighboring workspace does not activate it. Clicking the centered
preview or pressing Enter on it requests workspace activation. Clicking a
minimized item requests restore by stable window ID. Search spans workspaces
and focuses or restores the selected window. Successful action acknowledgement
closes the overview; an error leaves it open and visible. Duplicate activation
is blocked while a request is pending. Escape clears search before dismissing;
Ctrl+K and ordinary typing target search. Tab traverses controls. Arrow keys
browse workspaces outside search; Alt+Left/Right explicitly browse.

Window IDs preserve identity across snapshots. The browsed workspace remains
selected when compositor focus changes; removing that workspace selects the
active workspace, then the first remaining workspace. Snapshot delivery compares
full contents because older providers use window allocation counters as their
generation. A one-entry replacement mailbox retains the newest state. A lost
connection is reported with backoff, and reconnect publishes state even when its
generation/content matches the previous connection. Unsupported capacities are
reported rather than silently hiding windows (1,024 windows, 128 workspaces).

## Host API, resources and compatibility

`Application::uses_desktop` opts into the existing three bounded desktop
workers. New callbacks deliver snapshots, connection loss, preview updates and
action results. `preview_workspaces` restricts requests to visible workspaces;
this overview requests the selected workspace first and up to two neighbors.
`HostRequest::Desktop` adds an exhaustive enum variant. Applications should
maintain one outstanding desktop action and wait for its result. Requests are
collected after input and frame composition. Applications not opting in keep
no desktop workers. `Scene::set_focus` and `Key::K` are additive workspace APIs.
All consuming crates rebuild together at the existing unreleased 0.1.0 version.

The existing 500ms snapshot poll and error backoff remain. Previews refresh on
snapshot or browsed-workspace changes, not continuously; this intentionally
retains snapshot semantics. Requests/results are bounded, obsolete preview
results are ignored, and a full request queue retries after existing work wakes
the host. Closing drops worker senders before joining, preventing a full queue
from swallowing a stop command. In-flight requests still obey transport timeouts.
Idle frames reuse the display list. Hover/focus changes repaint without rebuilding
the overview; desktop/model, search, page and size changes rebuild its small
visible tree. Only the center and neighboring workspace cards are composed.

There is no configuration migration, new persistent state or compositor policy
change. Rollback restores the previous overview binary entry point and removes
its opt-in host callbacks together. No installed binary is replaced implicitly.

## Verification

Controller regressions cover workspace-scoped minimized items, pagination,
browse-versus-activate, stable selection on same-generation snapshots, restore
acknowledgement/failure, search/Escape, compact layouts, disconnect handling,
neighbor-only preview requests and idle display-list reuse. GPU captures and
live startup are separate checks; fixture snapshots do not establish real
compositor restoration or fresh workspace imagery.

A development fixture measured 2,000 alternating hover transitions at about
2.1 microseconds p95 CPU event/scene preparation, with one layout and seven text
measurements throughout (GPU encode/presentation excluded). A five-second idle
Wayland sample against a test desktop service held six threads, 71 FDs and about
188 MiB RSS, with two CPU ticks. These are initial baselines, not improvement
claims. The live host test delivered same-generation minimized-state changes,
dispatched one restore for the correct window ID and exited after acknowledgement.
Real Villain restoration remains unverified without its desktop socket.
