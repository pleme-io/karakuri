# Karakuri — Bevy ECS macOS Tiling Window Manager

## Build & Test

```bash
cargo build          # compile
cargo clippy         # lint (zero new warnings required)
cargo test           # 29 unit tests, deterministic (no platform deps)
cargo run            # launch (requires macOS Accessibility permissions)
```

## Architecture

### Pipeline

Every frame follows a strict five-stage pipeline. No system in a later stage
may send events consumed by an earlier stage within the same frame.

```
PreUpdate:   Event Ingestion    → pump_events, dispatch_toplevel_triggers
Update:      State Transitions  → window lifecycle, swipe, drag tracking
PostUpdate:  Layout Calculation → reload_guard_ticker, reshuffle_layout_strip
             Animation          → animate_windows, animate_resize_windows
             Rendering          → update_overlays, update_snap_preview
```

### Layout as Pure Computation

`position_layout_windows` is a pure function: given a window list, display
bounds, config, and viewport offset, it emits `RepositionMarker` /
`ResizeMarker` components. It never sends events, modifies focus, or touches
the platform API. Keep it this way.

### FSM for Interaction Modes

`InteractionMode` (Bevy `States` enum) gates system execution:

| State          | Active Systems                        |
|----------------|---------------------------------------|
| `Idle`         | All normal event processing           |
| `Dragging`     | Edge-snap preview, drag reposition    |
| `Swiping`      | Swipe momentum, viewport scrolling    |
| `MissionControl` | Event suppression                   |

When adding a new interaction mode, add it as a variant to `InteractionMode`
in `src/ecs/state.rs` and gate relevant systems with `in_state()` run
conditions. Prefer `States` transitions over manual boolean flags.

### Resource vs Component Decision

| Concept                | Pattern       | Why                                    |
|------------------------|---------------|----------------------------------------|
| Global singleton state | `Resource`    | Focus, config, interaction mode        |
| Per-window state       | `Component`   | Position, animation target, managed    |
| Transient guard/gate   | `Resource`    | ReloadGuard, SwipeContext              |
| Temporary markers      | `Component`   | RepositionMarker, ResizeMarker, Fresh  |

### SystemParam Split (Read/Write)

Configuration access is split into two SystemParams:
- `Configuration` — read-only (`Res<FocusContext>`), enables parallel scheduling
- `ConfigurationMut` — mutable (`ResMut<FocusContext>`), exclusive lock

Follow this pattern for any new shared state that has both readers and writers.

## Design Guidelines

### Guard/Gate Pattern for Cascading Events

When a single OS event can trigger cascading secondary events (space change →
workspace activation → focus change → reshuffle), use a typed Resource guard:

1. First trigger inserts the guard with a snapshot of current state
2. Cascade triggers check for guard, bump settle counter, skip redundant work
3. A ticker system counts down frames, then fires a single consolidated action
4. Guard is removed after the consolidated action completes

See `ReloadGuard` in `src/ecs/state.rs` for the canonical implementation.
Never use bare boolean flags for this — typed resources are self-documenting
and automatically cleaned up.

### Animation Strategy

- **Interpolation** for normal operations (focus switch, window swap)
- **Instant-snap** during reload guards and swipe tracking
- Animation systems check for guard/swipe resources to decide strategy
- When adding new animation, always handle the instant-snap path

Future direction: replace duration-based `animation_speed` with spring physics
(damping_ratio + stiffness). Springs handle mid-flight target changes naturally
without explicit interruption logic.

### Event Debouncing

Bevy's schedule guarantees each system runs at most once per frame. Combined
with the pipeline ordering, this naturally debounces within-frame cascades.
For cross-frame debouncing, use the guard/gate pattern above with a
`settle_frames` counter.

### Tree Normalization (if expanding layout model)

If adding nested container support beyond the current LayoutStrip model:
- **Flatten single-child containers** on window close (prevent ghost containers)
- **Enforce opposite-orientation nesting** (horizontal parent cannot nest
  horizontal child) — auto-flip parent orientation on violation
- These invariants should be structural (enforced in tree mutation methods),
  not checked after the fact

### Command Dispatch

Every user-visible action (keybinding, MCP command, script call) flows through
the same command enum and dispatch path. This makes testing deterministic —
test commands directly, not input device → event → command chains.

### Testing Principles

- **Deterministic FSM testing**: Test state transitions via `run_system_once`,
  not through complex event pipelines
- **No race conditions by construction**: Use type-system guarantees
  (`Res` vs `ResMut`) and structural constraints (`.chain()`)
- **Tests are platform-independent**: `pump_events` is a no-op in tests;
  events injected via `world.write_message::<Event>()`
- **Static TEST_MUTEX**: All integration tests serialize to prevent SIGABRT
  from parallel Bevy App initialization

## File Map

| Path | Purpose |
|------|---------|
| `src/ecs/state.rs` | Bevy States enums, context resources, guards |
| `src/ecs/systems.rs` | All frame-driven systems (layout, animation, event pump) |
| `src/ecs/triggers.rs` | Observer-driven triggers (focus, workspace, config, drag) |
| `src/ecs/params.rs` | Custom SystemParams (Windows, ActiveDisplay, Configuration) |
| `src/ecs.rs` | Entity helpers, component/marker definitions, app setup |
| `src/plugins/window.rs` | WindowPlugin — system registration and ordering |
| `src/config.rs` | TOML config parsing, keybinding resolution |
| `src/manager/` | Window, Display, LayoutStrip, Process abstractions |
| `src/platform/` | macOS platform layer (Accessibility API, gestures) |
| `src/commands/` | User command implementations (focus, swap, resize, etc.) |
| `src/overlay.rs` | Window border and dim-inactive overlay rendering |

## Patterns from the WM Ecosystem

These patterns informed karakuri's design and should guide future changes:

- **i3/sway**: Uniform container type, specification-driven command parser,
  two-phase render (compute geometry → batch platform calls)
- **AeroSpace**: Tree normalization, immutable layout snapshots, minimal
  private API usage (only `_AXUIElementGetWindow`)
- **Niri**: Scrollable tiling (viewport offset model — karakuri uses this),
  spring physics for animation, per-monitor structural independence
- **Komorebi**: `Ring<T>` focus tracking, three-channel event architecture,
  independent animation thread
- **Amethyst**: Layout as pure function (input: windows + rect → output: rects)
