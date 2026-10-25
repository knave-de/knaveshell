# Shell component boundaries

This repository owns the desktop-facing shell implementation and its UI
composition. This workspace owns the shell binary, Wayland hosting, application
discovery and icon resolution. The standalone Knave UI repository owns the UI
toolkit and wgpu renderer; this workspace consumes both at one pinned Git revision.

The shell:

- consumes typed settings projections and desktop contracts from
  Knave;
- uses an explicit Wayland client and the Knave desktop API;
- owns presentation and interaction state; and
- reports lifecycle failures to the session owner.

It does not own compositor layout, focus, window-management, or input policy.
It does not write the canonical user configuration or reach through a public
contract into Villain internals. A shared contract change updates both sides in
the same coordinated package change.
