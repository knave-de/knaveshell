# Knave Shell Agent Instructions

Knave Shell is the Rust/wgpu desktop-facing shell for the independent Knave
Desktop Environment. It owns shell presentation and interaction surfaces while
Knave owns settings/session orchestration and Villain owns compositor policy.
The repository is Cargo-only; it has no Qt, CMake, C ABI, or host-desktop
integration runtime.

## Startup and investigation

Before editing:

1. Read this file and the umbrella `knave/AGENTS.md` when working on a
   cross-repository change.
2. Read relevant README, architecture, API, and build documentation when it
   exists.
3. Inspect the complete owning module and search all of its consumers.
4. Check `git status` and preserve unrelated changes.
5. Classify whether the change affects UI, public desktop API, Wayland
   protocol, compositor IPC, configuration, build, packaging, or runtime behavior.
6. Write a short impact summary before changing a shared contract.

Do not implement a local-looking change until its cross-component effects have
been checked.

## Ownership and contracts

Knave owns the user-facing configuration and settings model. This repository
consumes Knave-owned settings only through typed APIs and public desktop
contracts. It must not introduce another user configuration source.

Villain remains the source of truth for workspace, window, focus, and preview
state. The shell consumes typed desktop contracts and must not duplicate
compositor policy.

Treat these as contracts:

- the Knave public desktop API payloads;
- shell roles and layer-shell behavior;
- workspace/window model fields;
- configuration and runtime environment variables.

Changing any of them requires consumer search, version/migration analysis,
tests, and documentation updates where user behavior changes.

## Build policy

Cargo is the only build system for this repository. Use the workspace profile
selected by the command and keep debug/release artifacts separate. The
installer always requires an explicit user, system, or custom prefix and must
not silently install into `/usr/local`.

After build changes, update `README.md` with only the commands and behavior
that actually exist.

## Code quality

Keep UI composition, rendering, Wayland integration, and desktop transport in
explicit crates. Do not mix rendering, transport, configuration, and
compositor policy in one module.

Use typed errors and honest unavailable/disconnected states. Do not fabricate
empty successful data when Villain is unavailable. Preserve reconnect behavior.

Review lifecycle and ownership carefully, especially for layer surfaces, GPU
resources, desktop API clients, snapshot workers, and spawned processes.

Comments should explain invariants or non-obvious reasons. Do not narrate
obvious C++, Rust, or QML code. Keep comments short.

## Performance and resource usage

Review resource behavior before adding QML timers, file or socket watchers,
IPC subscriptions, image providers, background tasks, threads, caches, or
parallel work.

- Give each activity an explicit owner, lifetime, cancellation path, and
  cleanup path for reload, disconnect, and shutdown.
- Bound subscriptions, model updates, preview/image caches, queues, retries,
  and concurrency. Do not create one worker or watcher per event without a
  measured bound.
- Prefer event-driven notifications, debouncing, batching, and backoff over
  polling or continuously waking event handlers.
- Measure CPU, resident memory, threads, file descriptors, and wakeups under
  idle, normal, and stress workloads when a change can affect them.

Compilation and functional tests do not establish acceptable shell performance.

## Documentation

Keep README instructions short and executable. Add architecture or migration
documentation only when a boundary or public contract changes. Do not write
long documentation for routine refactors or repeat source code in prose.

## Git and GitHub

- Preserve unrelated dirty changes.
- Create a new focused branch for every task.
- Use Conventional Commits.
- Commit every coherent implementation slice.
- Link dependent cross-repository pull requests.
- Describe affected contracts, consumers, compatibility, verification,
  rollout, and rollback in cross-repository pull requests.
- Never merge, squash-merge, or otherwise integrate a branch or pull request
  unless the user explicitly requests the merge. Creating or approving a pull
  request, passing checks, or repository conventions do not authorize a merge.
- When the user explicitly requests a merge, follow their specified method; if
  they do not specify one, use squash merge.

## Verification

For desktop UI/runtime features, follow `../knave/docs/desktop-testing.md` and
the umbrella Knave desktop-feature verification instructions. Build the affected
workspace binaries, exercise the feature in the isolated Winit harness, verify
state/output and screenshots, and record an MP4 demonstration for the user.
Provide the video link and assertions in the final response. Leave a requested
interactive handoff running with its VNC/SSH connection details; stop sessions
used only for automated checks. This does not establish direct-TTY/GPU behavior.

For Rust changes, run:

```console
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

A successful compile does not prove layer-shell, focus restoration, GPU
behavior, direct-TTY startup, or installed-binary startup. Run the relevant
Wayland/GPU smoke tests separately and report direct-TTY coverage explicitly.

Before completion, inspect the final diff, run `git diff --check`, check
generated files, and report anything not tested.
