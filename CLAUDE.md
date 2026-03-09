# Ayatsuri — Bevy ECS macOS Window Manager & Status Bar

## Build & Test

```bash
cargo build          # compile (zero warnings required)
cargo test           # 175 unit tests, deterministic (no platform deps)
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

- **Spring physics** (critically-damped harmonic oscillator) for normal
  operations — `SpringState` component tracks per-window velocity
- **Instant-snap** during reload guards and swipe tracking
- Animation systems check for guard/swipe resources to decide strategy
- When adding new animation, always handle the instant-snap path
- Default parameters: stiffness=800, damping_ratio=1.0, epsilon=0.5px
- Mid-flight retargeting preserves velocity for smooth trajectory changes
- Pure spring math lives in `logic/spring.rs` with 11 unit tests

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

### Zero-Unwrap Policy

Production code contains zero `unwrap()` or `expect()` calls. All error paths
use one of:
- `Result` propagation with `?` operator
- `let Some(...) = expr else { return ... }` guards
- Safe casts (`as usize`, `as u32`) where the value is known to fit
- Compile-time literals (`u32::from_be_bytes(*b"psn ")`)
- RwLock poison recovery (`.into_inner()` on poisoned lock)

### Testing Principles

- **Deterministic FSM testing**: Test state transitions via `run_system_once`,
  not through complex event pipelines
- **No race conditions by construction**: Use type-system guarantees
  (`Res` vs `ResMut`) and structural constraints (`.chain()`)
- **Tests are platform-independent**: `pump_events` is a no-op in tests;
  events injected via `world.write_message::<Event>()`
- **Static TEST_MUTEX**: All integration tests serialize to prevent SIGABRT
  from parallel Bevy App initialization

## Pure Logic Extraction (`src/logic/`)

All decision-making and math that can be tested without Bevy or macOS
dependencies lives in `src/logic/`. ECS systems and triggers call into
these modules — they never contain the algorithm inline.

| Module | Functions | Tests |
|--------|-----------|-------|
| `logic/snap.rs` | `detect_snap_zone`, `snap_frame` | 16 |
| `logic/navigation.rs` | `window_in_direction`, `display_in_direction` | 24 |
| `logic/swipe.rs` | `smooth_velocity`, `velocity_to_pixel_shift`, `clamp_viewport_offset`, `below_stop_threshold`, `delta_to_shift` | 22 |
| `logic/drag.rs` | `clamp_origin_to_bounds`, `offset_frame_within_bounds` | 16 |
| `logic/spring.rs` | `step` (damped harmonic oscillator) | 11 |
| `logic/layout.rs` | `compute_final_frames` (coordinate transforms, sliver logic) | 17 |
| `logic/bar_layout.rs` | `compute_bar_layout` (left/center/right/q/e stacking) | 7 |

**Convention**: When adding new behavior, write the pure function in `logic/`
first with unit tests, then call it from the ECS layer. The ECS layer handles
entity queries, resource access, and command dispatch — never branching logic
or math.

## Platform Isolation (trait boundaries)

All macOS system interactions are behind trait boundaries for testability:

| Trait | macOS Impl | Mock | Purpose |
|-------|------------|------|---------|
| `WindowManagerApi` | `WindowManagerOS` | `MockWindowManager` | SkyLight, display/space queries, cursor |
| `WindowApi` | `WindowOS` | `MockWindow` | AX accessibility (frame, focus, resize) |
| `ProcessApi` | `ProcessOS` | `MockProcess` | Carbon process events, NSRunningApplication |
| `ApplicationApi` | `ApplicationOS` | `MockApplication` | AX observer, window discovery |
| `OverlayApi` | `OverlayManager` | (use `Option<>` guard) | Cocoa overlay windows (dim, border, snap preview) |

**Event isolation**: Platform handlers (InputHandler, DisplayHandler, ProcessHandler,
WorkspaceObserver) fire events through an MPSC channel. ECS systems only consume
`Event` messages — never call platform APIs directly. Tests inject events via
`world.write_message::<Event>()`.

**NonSend resources**: `OverlayManager` and `PlatformCallbacks` are stored as
`Option<NonSendMut<T>>`. Systems short-circuit with `return` when absent (in tests).
The `OverlayApi` trait enables future mock injection if needed.

**Adapter pattern for Display lookups**: `logic/navigation::display_in_direction`
takes `&[IRect]` and returns `Option<usize>`. Call sites in `commands.rs`
collect Display refs into a Vec, extract bounds, call the pure function, and
index back to get the Display.

## File Map

| Path | Purpose |
|------|---------|
| `src/logic/` | Pure testable logic (snap, navigation, swipe, drag, spring, layout) |
| `src/ecs/state.rs` | Bevy States enums, context resources, guards |
| `src/ecs/systems/` | Frame-driven systems (mod.rs + animation.rs + overlay.rs) |
| `src/ecs/triggers.rs` | Observer-driven triggers (focus, workspace, config, drag) |
| `src/ecs/params.rs` | Custom SystemParams (Windows, ActiveDisplay, Configuration) |
| `src/ecs.rs` | Entity helpers, component/marker definitions, app setup |
| `src/plugins/window.rs` | WindowPlugin — system registration and ordering |
| `src/plugins/status_bar/` | StatusBarPlugin — bar window, items, layout, rendering |
| `src/config.rs` | TOML/YAML config parsing, keybinding resolution |
| `src/manager/` | Window, Display, LayoutStrip, Process abstractions |
| `src/platform/` | macOS platform layer (Accessibility API, gestures) |
| `src/commands.rs` | User command implementations (focus, swap, resize, etc.) |
| `src/overlay.rs` | Window border and dim-inactive overlay rendering |

## Status Bar (`src/plugins/status_bar/`)

Ayatsuri includes a built-in GPU-rendered status bar (inspired by SketchyBar) that
replaces or overlays the macOS menu bar. It is a first-class ECS feature — bar items
are Bevy entities with components, updated by systems, and reactive to window manager
state.

### Architecture

```
┌─────────────────────── Status Bar Architecture ───────────────────────┐
│                                                                       │
│  Config (YAML)                                                        │
│    └→ StatusBarConfig ── parsed into ECS Resources on startup/reload  │
│                                                                       │
│  ECS Entities (one per bar item)                                      │
│    ├── BarItemComponent  { id, position, script, update_freq }        │
│    ├── BarItemState      { icon_text, label_text, colors, hidden }    │
│    └── BarItemGeometry   { x, y, width, height, padding }            │
│                                                                       │
│  Systems (PostUpdate, after window layout)                            │
│    ├── update_bar_items  — runs scripts, polls event providers        │
│    ├── layout_bar_items  — pure geometry: left|center|right stacking  │
│    └── render_bar        — draws to NSWindow via CoreGraphics         │
│                                                                       │
│  Platform (NonSend, main thread)                                      │
│    ├── StatusBarWindow   — borderless NSWindow at screen top           │
│    ├── CoreGraphics      — CGContext text/rect/icon drawing            │
│    └── Mouse tracking    — NSTrackingArea for click/hover/scroll       │
│                                                                       │
│  Scripting (Rhai + MCP)                                               │
│    ├── bar_set(id, props) — update item properties from scripts       │
│    ├── bar_add(id, pos)   — add item at runtime                       │
│    ├── bar_remove(id)     — remove item at runtime                    │
│    └── MCP tools          — query/mutate bar state from Claude Code   │
│                                                                       │
└───────────────────────────────────────────────────────────────────────┘
```

### Design Decisions

**NSWindow + CoreGraphics, not SkyLight private APIs.** SketchyBar uses SLS
private APIs (`SLSNewWindowWithOpaqueShapeAndContext`) for per-item windows.
We use a single borderless `NSWindow` per display drawn with `CGContext`. Reasons:
- objc2 provides safe bindings for NSWindow/CoreGraphics — no raw FFI needed
- Single window = simpler compositing, fewer resources
- CoreGraphics text rendering (Core Text) handles fonts, emoji, SF Symbols
- Background blur via `NSVisualEffectView` (public API, same visual result)
- Matches ayatsuri's existing overlay window pattern (`make_overlay_window`)

**Bar items as ECS entities.** Each item is a Bevy entity with components:
- Enables the same query/system patterns used for window management
- Items react to WM events naturally (workspace change → update space indicator)
- MCP can query bar state through the existing snapshot infrastructure
- Hot-reload: config changes diff against existing entities, add/remove/update

**Event-driven updates, not polling.** Items subscribe to events:
- WM events (focus change, space change, display change) — already in ECS
- System events (volume, brightness, power, wifi, media) — new platform observers
- Timer events (clock, periodic script execution) — Bevy timer resources
- Script triggers (user-defined custom events) — via Rhai or CLI

### Item Types

| Type | Description |
|------|-------------|
| `item` | Standard element: icon + label + background |
| `space` | Workspace indicator (reactive to space changes) |
| `bracket` | Groups items with shared background |
| `graph` | Rolling line chart (CPU, memory, network) |
| `slider` | Interactive progress bar (volume, brightness) |
| `alias` | Mirror of a native macOS menu bar item |

### Positions

Five positions on the bar: `left`, `right`, `center`, `q` (left of notch),
`e` (right of notch). Items stack from the edge inward within each group.

### Drawing Primitives (per item)

Each item has three visual layers: **icon** (text/glyph), **label** (text),
**background** (rounded rect). The background supports:
- Fill color (ARGB hex) with alpha transparency
- Border color + width + corner radius
- Padding (left/right/top/bottom)
- Image (loaded from file)

Text supports any installed font (Nerd Fonts, SF Symbols, system fonts).
Font spec format: `"Font Name:Style:Size"` (e.g., `"Hack Nerd Font:Bold:14.0"`).

### Mouse Interaction

NSTrackingArea on the bar window detects hit regions per-item:
- **Click**: left/right/other button + modifier keys → triggers item script
- **Scroll**: delta forwarded to item (volume slider, brightness)
- **Hover**: mouse.entered/mouse.exited events per item

### Animation

Bar items use the same spring animation system as windows. Animatable
properties: position, color (per-channel interpolation), opacity, width.

### Popup Menus

Items can have popup children — a secondary bar window anchored below the
item. Popups contain their own items (recursive). Triggered by click or
hover. Dismissed on click-outside or escape.

### Configuration (YAML)

```yaml
status_bar:
  enabled: true
  position: top            # top | bottom
  height: 28
  blur_radius: 20          # background blur (0 = opaque)
  color: "0xCC1e1e2e"      # ARGB hex (CC = 80% opacity)
  border_color: "0xFF313244"
  border_width: 1
  corner_radius: 0
  font: "Hack Nerd Font:Regular:14.0"
  icon_font: "Hack Nerd Font:Regular:16.0"
  padding_left: 8
  padding_right: 8
  y_offset: 0
  topmost: false           # above all windows?
  sticky: true             # visible on all spaces?
  display: "all"           # all | main | <display-index>
  notch_width: 220         # space reserved for MacBook notch
  hide_macos_menubar: true # auto-hide the native menu bar

  defaults:                # default properties for all items
    icon_color: "0xFFcdd6f4"
    label_color: "0xFFcdd6f4"
    background_color: "0x00000000"
    padding_left: 6
    padding_right: 6

  items:
    - id: spaces
      type: space
      position: left
      spaces: [1, 2, 3, 4, 5]
      icon_font: "Hack Nerd Font:Bold:16.0"
      selected_color: "0xFFcba6f7"
      background_corner_radius: 6
      background_height: 22

    - id: front_app
      type: item
      position: left
      subscribe: [front_app_switched]
      icon: "app"           # special: uses app icon
      label_font: "Hack Nerd Font:Bold:13.0"

    - id: clock
      type: item
      position: right
      update_freq: 1         # seconds
      script: "date '+%H:%M:%S'"
      icon: ""

    - id: battery
      type: item
      position: right
      subscribe: [power_source_change]
      script: "~/.config/ayatsuri/plugins/battery.sh"

    - id: cpu
      type: graph
      position: right
      width: 60
      graph_color: "0xFFa6e3a1"
      graph_fill_color: "0x33a6e3a1"
      update_freq: 2

    - id: volume
      type: slider
      position: right
      subscribe: [volume_change]
      slider_color: "0xFFcba6f7"
      slider_knob_color: "0xFFf5e0dc"
```

### Implementation Phases

**Phase 1 — Core rendering + static items** (current target)
- `StatusBarWindow` — NSWindow creation with background blur
- CoreGraphics drawing: text (Core Text), rounded rects, colors
- Layout system: left/center/right item stacking with padding
- Mouse tracking: click detection per item region
- Config parsing: `status_bar` section in ayatsuri.yaml
- Built-in items: clock, front_app (from existing WM state)

**Phase 2 — Event system + scripting**
- System event observers: volume, brightness, power, wifi, media
- Script execution: fork+exec for shell scripts, env vars ($NAME, $SENDER)
- Rhai API: `bar_set()`, `bar_add()`, `bar_remove()`
- Timer-based updates (update_freq)
- Item subscriptions to named events

**Phase 3 — Advanced items + popups**
- Space indicators (reactive to workspace changes — data already in ECS)
- Graph items (rolling line chart with circular buffer)
- Slider items (interactive, draggable)
- Popup windows (child items, anchored to parent)
- Alias items (mirror native menu bar items via CGWindowListCopyWindowInfo)

**Phase 4 — Polish + ecosystem**
- Animation (spring physics for item transitions, color interpolation)
- Multiple bar instances (per-display)
- MCP tools: `get_bar_state`, `set_bar_item`, `add_bar_item`
- Nix HM module: `services.ayatsuri.statusBar` config options
- Default plugins: battery, cpu, memory, network, media, k8s-context

### File Structure

| Path | Purpose |
|------|---------|
| `src/plugins/status_bar/mod.rs` | StatusBarPlugin — system registration |
| `src/plugins/status_bar/config.rs` | Config parsing for status_bar section |
| `src/plugins/status_bar/components.rs` | ECS components (BarItem, BarState, BarGeometry) |
| `src/plugins/status_bar/layout.rs` | Pure layout logic (position calculation) |
| `src/plugins/status_bar/render.rs` | CoreGraphics drawing (text, rects, icons) |
| `src/plugins/status_bar/window.rs` | NSWindow creation and management (NonSend) |
| `src/plugins/status_bar/mouse.rs` | NSTrackingArea hit detection |
| `src/plugins/status_bar/events.rs` | System event observers (volume, wifi, etc.) |
| `src/plugins/status_bar/items/` | Built-in item implementations (clock, battery, etc.) |
| `src/logic/bar_layout.rs` | Pure bar layout math (testable without platform) |

### Key Constraints

- **No unwrap/expect** — follow zero-unwrap policy
- **Pure logic in `logic/`** — bar layout math is testable without Bevy or macOS
- **NonSend for platform** — StatusBarWindow is main-thread only
- **Hot-reload** — config changes update items without restart
- **objc2 only** — no raw FFI, no `unsafe` blocks, use objc2 safe bindings
- **Event-driven** — no polling loops, use subscriptions and timers

## Keybinding System (awase integration)

Ayatsuri uses **awase** (`github.com/pleme-io/awase`) as the single, centralized
hotkey library. There is **no** separate keybinding implementation in ayatsuri —
all hotkey parsing, matching, mode switching, and platform event interception live
in awase. Ayatsuri consumes awase types via its `Cargo.toml` dependency.

### Why awase, not ad-hoc

Every pleme-io app that needs hotkeys (tobirato, ayatsuri, future apps) uses awase.
No duplication. The library is enriched once, consumed everywhere.

### Architecture

```
┌─────────────────── Keybinding Architecture ──────────────────────┐
│                                                                   │
│  Config (YAML)                                                    │
│    └→ bindings: { "cmd+h": "window_focus_west" }                 │
│    └→ modes: { resize: { "h": "window_shrink_west" } }          │
│    └→ conditions: { app: "com.apple.Terminal" }                   │
│                                                                   │
│  awase (library — lives in github.com/pleme-io/awase)            │
│    ├── Hotkey       — modifier + key combination                  │
│    ├── KeyChord     — multi-step key sequence (leader → follower) │
│    ├── KeyMode      — named mode with its own binding set         │
│    ├── BindingMap   — mode-aware hotkey → action lookup           │
│    ├── Condition    — per-app, per-title, per-display filters     │
│    ├── HotkeyManager trait — platform-agnostic registration       │
│    ├── CgEventTapManager  — macOS CGEventTap implementation       │
│    └── Key/Modifiers      — exhaustive key + modifier enums       │
│                                                                   │
│  ayatsuri (consumer)                                              │
│    ├── config.rs    — parses YAML bindings into awase types       │
│    ├── platform/input.rs — CgEventTapManager event loop           │
│    ├── ecs/        — HotkeyState Resource, keybind system         │
│    └── commands.rs — Command enum dispatched by bindings          │
│                                                                   │
│  Rhai scripting (via soushi)                                      │
│    ├── bind("cmd+k", || focus_north())                           │
│    ├── unbind("cmd+k")                                           │
│    ├── set_mode("resize")                                        │
│    └── get_bindings() → dynamic binding query                    │
│                                                                   │
│  MCP tools                                                        │
│    ├── list_bindings    — query all active bindings               │
│    ├── set_binding      — add/replace a binding at runtime        │
│    ├── remove_binding   — remove a binding at runtime             │
│    └── set_mode         — switch keybinding mode                  │
│                                                                   │
└───────────────────────────────────────────────────────────────────┘
```

### awase Feature Requirements

These features must be implemented in awase (the library), not in ayatsuri.
They are listed here to ensure completeness during implementation.

#### Core Types (already exist, need enrichment)

| Feature | Status | Notes |
|---------|--------|-------|
| `Key` enum (A-Z, 0-9, F1-F12, arrows, space, return, etc.) | EXISTS | Add: Home, End, PageUp, PageDown, PrintScreen, Insert, Pause, media keys, mouse buttons |
| `Modifiers` bitmask (CMD, CTRL, ALT, SHIFT) | EXISTS | Add: FN, HYPER (Cmd+Ctrl+Alt+Shift), CAPS_LOCK |
| `Hotkey` (modifiers + key) | EXISTS | No changes needed |
| `Hotkey::parse("cmd+space")` | EXISTS | Also support skhd format "cmd - space" |
| `HotkeyManager` trait (register/unregister) | EXISTS | Extend with mode-aware API |
| `NoopManager` for testing | EXISTS | No changes needed |

#### New Key Types (add to awase)

```rust
// Media / special keys
Key::VolumeUp, Key::VolumeDown, Key::Mute,
Key::BrightnessUp, Key::BrightnessDown,
Key::PlayPause, Key::NextTrack, Key::PreviousTrack,
Key::Home, Key::End, Key::PageUp, Key::PageDown,
Key::PrintScreen, Key::Insert, Key::Pause,
Key::Grave, Key::Minus, Key::Equal,
Key::LeftBracket, Key::RightBracket, Key::Backslash,
Key::Semicolon, Key::Quote, Key::Comma, Key::Period, Key::Slash,
Key::NumpadAdd, Key::NumpadSubtract, Key::NumpadMultiply,
Key::NumpadDivide, Key::NumpadDecimal, Key::NumpadEnter,
Key::Numpad0..Key::Numpad9,
Key::CapsLock, Key::NumLock, Key::ScrollLock,

// Mouse buttons (for mouse bindings)
Key::MouseLeft, Key::MouseRight, Key::MouseMiddle,
Key::MouseButton4, Key::MouseButton5,

// Modifier aliases
Modifiers::FN       // Fn key
Modifiers::HYPER    // Cmd+Ctrl+Alt+Shift combo
```

#### Mode System (add to awase — inspired by skhd)

skhd supports named modes with independent binding sets. Mode switching is
itself a binding action.

```rust
/// A named keybinding mode with its own binding set.
pub struct KeyMode {
    /// Mode name (e.g., "default", "resize", "launch").
    pub name: String,
    /// Bindings active in this mode.
    pub bindings: HashMap<Hotkey, Action>,
    /// Whether unmatched keys pass through to the app (true) or are consumed (false).
    pub passthrough: bool,
}

/// Action triggered by a keybinding.
pub enum Action {
    /// Named command string (consumer interprets, e.g., "window_focus_west").
    Command(String),
    /// Switch to a different mode.
    ModeSwitch(String),
    /// Execute shell command.
    Exec(String),
    /// Rhai script evaluation.
    Script(String),
    /// Chain: execute action then switch mode (e.g., do something then return to default).
    Chain(Vec<Action>),
}
```

YAML config:
```yaml
modes:
  default:
    passthrough: true
    bindings:
      "cmd+h": window_focus_west
      "cmd+j": window_focus_south
      "cmd+k": window_focus_north
      "cmd+l": window_focus_east
      "ctrl+alt+r": { mode: resize }
      "ctrl+alt+l": { mode: launch }

  resize:
    passthrough: false
    bindings:
      "h": window_shrink_west
      "l": window_grow_east
      "j": window_grow_south
      "k": window_shrink_north
      "escape": { mode: default }
      "return": { mode: default }

  launch:
    passthrough: false
    bindings:
      "t": { exec: "open -a Terminal" }
      "b": { exec: "open -a 'Arc'" }
      "escape": { mode: default }
```

Nix HM options:
```nix
bindings.modes = mkOption {
  type = types.attrsOf (types.submodule {
    options = {
      passthrough = mkOption { type = types.bool; default = true; };
      bindings = mkOption {
        type = types.attrsOf (types.either types.str (types.submodule { ... }));
        default = {};
      };
    };
  });
};
```

#### Key Chords / Sequences (add to awase)

Multi-key sequences like tmux `prefix → key` or Emacs `C-x C-f`.
A leader key starts a chord; the next key completes it.

```rust
/// A two-step key chord: leader + follower.
pub struct KeyChord {
    pub leader: Hotkey,
    pub follower: Hotkey,
    /// Timeout in milliseconds before the chord is cancelled.
    pub timeout_ms: u32,
}
```

YAML config:
```yaml
chords:
  "ctrl+a":
    timeout_ms: 1000
    bindings:
      "c": { exec: "open -a Terminal" }
      "n": window_focus_next
      "p": window_focus_prev
      "1": { exec: "ayatsuri --space 1" }
```

Implementation: when the leader key fires, enter a transient "chord pending"
state. The next keypress within `timeout_ms` is matched against the chord's
follower bindings. If no match or timeout, the leader key event is optionally
passed through.

#### Conditional Bindings (add to awase)

Per-app, per-window-title, per-display conditions that filter when a binding
is active. Inspired by Karabiner-Elements `conditions` and skhd `--blacklist`.

```rust
/// Conditions that gate when a binding is active.
pub struct Condition {
    /// Active only when the focused app matches (bundle_id regex).
    pub app: Option<String>,
    /// Active only when the focused app does NOT match.
    pub app_exclude: Option<String>,
    /// Active only when the window title matches (regex).
    pub title: Option<String>,
    /// Active only on the specified display index.
    pub display: Option<u32>,
}
```

YAML config:
```yaml
bindings:
  "cmd+h":
    action: window_focus_west
    conditions:
      app_exclude: "com.apple.Terminal"  # don't override Terminal's cmd+h
```

#### Passthrough vs Consume (add to awase)

Each binding specifies whether the key event is consumed (not forwarded to the
focused app) or passed through. Default: consumed (same as skhd).

```rust
pub struct Binding {
    pub hotkey: Hotkey,
    pub action: Action,
    pub consume: bool,     // default: true
    pub condition: Option<Condition>,
}
```

#### Key Remapping (add to awase — inspired by Karabiner)

Simple key-to-key remapping at the CGEventTap level. Examples:
- Caps Lock → Escape
- Right Option → Hyper
- Fn+HJKL → Arrow keys

```rust
pub struct KeyRemap {
    pub from: Hotkey,
    pub to: Hotkey,
    /// Optional condition (per-app, etc.).
    pub condition: Option<Condition>,
}
```

YAML config:
```yaml
remaps:
  - from: caps_lock
    to: escape
  - from: "fn+h"
    to: left
  - from: "fn+j"
    to: down
  - from: "fn+k"
    to: up
  - from: "fn+l"
    to: right
```

#### macOS Platform Backend (add to awase)

The `CgEventTapManager` implements `HotkeyManager` using `CGEventTapCreate`.
This code currently lives ad-hoc in `ayatsuri/src/platform/input.rs` and must
be extracted into awase as a feature-gated module.

```rust
// In awase, behind #[cfg(target_os = "macos")] or feature = "macos"
pub struct CgEventTapManager {
    tap_port: Option<CFRetained<CFMachPort>>,
    bindings: BindingMap,
    current_mode: String,
    chord_state: Option<ChordPending>,
    callback: Box<dyn Fn(MatchResult) + Send>,
}

/// Result of matching a key event against bindings.
pub enum MatchResult {
    /// Binding matched — return the action and whether to consume.
    Matched { action: Action, consume: bool },
    /// Chord leader matched — waiting for follower.
    ChordPending { leader: Hotkey, timeout_ms: u32 },
    /// No match — pass through.
    NoMatch,
}
```

This extraction eliminates the duplicate keybinding logic. ayatsuri's
`InputHandler::handle_keypress` becomes a thin wrapper that converts
`MatchResult` → `Event::Command`.

#### Virtual Keycode Mapping (add to awase)

A `Key → u16` mapping table for macOS virtual keycodes. Currently scattered
across ayatsuri's `generate_virtual_keymap()` and `literal_keycode()`.
Centralize in awase:

```rust
/// Map an awase Key to a macOS virtual keycode (u16).
pub fn key_to_macos_keycode(key: Key) -> Option<u16>;

/// Map a macOS virtual keycode to an awase Key.
pub fn macos_keycode_to_key(code: u16) -> Option<Key>;

/// Map CGEventFlags to awase Modifiers.
pub fn cg_flags_to_modifiers(flags: CGEventFlags) -> Modifiers;
```

#### Synthesized Key Events (add to awase)

Programmatically send keystrokes to the focused app. Used for text expansion,
macros, and scripting.

```rust
pub fn send_key_event(key: Key, modifiers: Modifiers, key_down: bool);
pub fn type_text(text: &str); // synthesize a string as key events
```

#### Conflict Detection (add to awase)

When registering bindings, detect conflicts:
- Two bindings for the same hotkey in the same mode → error
- A binding that shadows a system shortcut → warning
- A chord leader that conflicts with a regular binding → error

```rust
pub struct ConflictReport {
    pub conflicts: Vec<ConflictEntry>,
}

pub struct ConflictEntry {
    pub hotkey: Hotkey,
    pub existing: String,  // description of existing binding
    pub new: String,        // description of conflicting binding
}
```

#### Runtime Management API (add to awase)

```rust
pub trait HotkeyManager: Send + Sync {
    fn register(&mut self, id: u32, hotkey: Hotkey) -> Result<(), AwaseError>;
    fn unregister(&mut self, id: u32) -> Result<(), AwaseError>;

    // New methods:
    fn set_mode(&mut self, mode: &str) -> Result<(), AwaseError>;
    fn current_mode(&self) -> &str;
    fn load_config(&mut self, config: &BindingConfig) -> Result<ConflictReport, AwaseError>;
    fn list_bindings(&self) -> Vec<(Hotkey, String)>; // hotkey → action description
    fn register_chord(&mut self, id: u32, chord: KeyChord) -> Result<(), AwaseError>;
    fn register_remap(&mut self, remap: KeyRemap) -> Result<(), AwaseError>;
}
```

### ayatsuri Integration Plan

#### Phase 1: Extract and Replace

1. **Move virtual keycode tables** from `ayatsuri/src/config.rs`
   (`generate_virtual_keymap`, `literal_keycode`) into awase as
   `key_to_macos_keycode()` / `macos_keycode_to_key()`.

2. **Move modifier conversion** from `ayatsuri/src/platform/input.rs`
   (`MODIFIER_MASKS` array, `parse_modifiers`) into awase as
   `cg_flags_to_modifiers()`.

3. **Replace ayatsuri's `Keybinding` struct** with `awase::Hotkey` +
   `awase::Action`. Delete `ayatsuri::config::Keybinding`,
   `ayatsuri::platform::Modifiers` — use `awase::Modifiers` everywhere.

4. **Replace `find_keybind(keycode, mask)`** with awase's `BindingMap::match_key()`.

5. **Replace `resolve_keybindings()`** with awase's `BindingConfig::parse()`.

#### Phase 2: Mode System + Chords

1. Add `KeyMode`, `BindingMap`, `KeyChord` to awase.
2. Add `HotkeyState` ECS Resource to ayatsuri wrapping `BindingMap`.
3. Add `Command::SetMode(String)` to ayatsuri's command enum.
4. YAML config: `modes:` section parsed by awase.
5. Rhai API: `set_mode("resize")`, `bind("cmd+k", callback)`.

#### Phase 3: Conditions + Remapping

1. Add `Condition` to awase with app/title/display filtering.
2. Wire focused app bundle_id from ECS into awase condition evaluation.
3. Add `KeyRemap` support in `CgEventTapManager`.
4. YAML config: `remaps:` and per-binding `conditions:`.

#### Phase 4: Platform Extraction

1. Extract `CgEventTapManager` from ayatsuri into awase behind
   `#[cfg(target_os = "macos")]` feature gate.
2. ayatsuri's `InputHandler` becomes thin: create `CgEventTapManager`,
   convert `MatchResult` → `Event::Command`.
3. Other apps (tobirato) can now use the same `CgEventTapManager`.

### Configuration (YAML)

Full keybinding config example:

```yaml
# Simple bindings (default mode, consumed, no conditions)
bindings:
  window_focus_west: "cmd+h"
  window_focus_east: "cmd+l"
  window_focus_north: "cmd+k"
  window_focus_south: "cmd+j"
  quit: "ctrl+alt+q"
  reload_config: "ctrl+alt+r"

# Named exec commands
execs:
  terminal: "open -a Terminal"
  browser: "open -a 'Arc'"

# Exec bindings reference the execs map
bindings:
  exec_terminal: "ctrl+alt+t"
  exec_browser: "ctrl+alt+b"

# Mode system
modes:
  default:
    passthrough: true
    bindings:
      "ctrl+alt+r": { mode: resize }
      "ctrl+alt+l": { mode: launch }

  resize:
    passthrough: false
    bindings:
      "h": window_shrink_west
      "l": window_grow_east
      "j": window_grow_south
      "k": window_shrink_north
      "=": window_equalize
      "escape": { mode: default }

  launch:
    passthrough: false
    bindings:
      "t": { exec: "open -a Terminal" }
      "b": { exec: "open -a 'Arc'" }
      "f": { exec: "open -a Finder" }
      "escape": { mode: default }

# Key chords (leader → follower sequences)
chords:
  "ctrl+a":
    timeout_ms: 1000
    bindings:
      "c": { exec: "open -a Terminal" }
      "1": workspace_1
      "2": workspace_2

# Key remaps
remaps:
  - from: caps_lock
    to: escape
  - from: "fn+h"
    to: left
  - from: "fn+j"
    to: down
  - from: "fn+k"
    to: up
  - from: "fn+l"
    to: right

# Conditional bindings
conditional_bindings:
  - hotkey: "cmd+h"
    action: window_focus_west
    conditions:
      app_exclude: "com.apple.Terminal|com.mitchellh.ghostty"
```

### Nix HM Module Options

```nix
bindings = mkOption {
  type = types.attrsOf (types.either types.str (types.listOf types.str));
  default = {};
  description = "Keybindings: command_name = 'modifier+modifier-key'";
  example = {
    window_focus_west = "cmd - h";
    window_focus_east = "cmd - l";
    quit = "ctrl + alt - q";
  };
};

modes = mkOption {
  type = types.attrsOf (types.submodule {
    options = {
      passthrough = mkOption { type = types.bool; default = true; };
      bindings = mkOption {
        type = types.attrsOf types.anything;
        default = {};
      };
    };
  });
  default = {};
};

remaps = mkOption {
  type = types.listOf (types.submodule {
    options = {
      from = mkOption { type = types.str; };
      to = mkOption { type = types.str; };
    };
  });
  default = [];
};
```

### Security & Permissions

- **CGEventTap** requires Accessibility permissions (System Preferences →
  Privacy & Security → Accessibility). ayatsuri already requests this.
- **Input Monitoring** permission is also needed on macOS Monterey+.
- awase should expose a `check_permissions()` function that queries
  `AXIsProcessTrusted()` and `IOHIDCheckAccess()`.
- On permission denial, awase returns `AwaseError::PermissionDenied` —
  ayatsuri displays a user-facing notification via tsuuchi.

### Key Constraints

- **Single source of truth** — ALL hotkey logic lives in awase, not ayatsuri
- **No duplication** — ayatsuri must NOT have its own key/modifier types
- **Pure logic testable** — mode matching, conflict detection, chord state
  machine logic lives in awase with unit tests (no platform deps)
- **Platform code feature-gated** — `CgEventTapManager` behind
  `#[cfg(target_os = "macos")]` so awase compiles on any platform
- **Hot-reload** — binding changes via config reload or runtime API
  without restart (existing shikumi/ArcSwap pattern)

## Patterns from the WM Ecosystem

These patterns informed ayatsuri's design and should guide future changes:

- **i3/sway**: Uniform container type, specification-driven command parser,
  two-phase render (compute geometry → batch platform calls)
- **AeroSpace**: Tree normalization, immutable layout snapshots, minimal
  private API usage (only `_AXUIElementGetWindow`)
- **Niri**: Scrollable tiling (viewport offset model — ayatsuri uses this),
  spring physics for animation, per-monitor structural independence
- **Komorebi**: `Ring<T>` focus tracking, three-channel event architecture,
  independent animation thread
- **Amethyst**: Layout as pure function (input: windows + rect → output: rects)
- **SketchyBar**: Event-driven status bar with per-item scripting, animation
  curves, popup menus, graph/slider components, Mach IPC command interface,
  background blur via SLS APIs, five-position layout (left/center/right/q/e)
- **skhd**: Mode-based hotkey daemon, CGEventTap interception, config-driven
  binding resolution, blacklist system, passthrough control
- **Karabiner-Elements**: Low-level key remapping, complex modifications with
  conditions, device-specific rules, virtual keyboard abstraction

---

## Competitive Position

### Landscape

| Competitor | Language | Architecture | Extension Model | Strengths | Weaknesses |
|------------|----------|-------------|-----------------|-----------|------------|
| **yabai** | C | Ad-hoc state, signal-based | Shell CLI (`yabai -m`) | BSP tiling, SIP features, mature | C state management, SIP requirement for advanced features, no scripting engine |
| **Amethyst** | Swift | Layout algorithms, no tree | None (fixed layouts) | Simple, no SIP, automatic tiling | Not programmable, limited layouts, no plugins |
| **AeroSpace** | Swift | Tree-based (i3-like) | Config file only | i3 semantics, workspaces, no SIP | Tree-only layout, no scripting, no MCP, no status bar |
| **Rectangle** | Swift | Snap zones | None | Simple keyboard shortcuts, snap zones | No tiling, no scripting, basic |
| **Hammerspoon** | Obj-C + Lua | Lua scripting runtime | Spoons (Lua plugins) | Extremely flexible, automation beyond WM | Lua performance, free-form config, no type safety, no MCP |

### Ayatsuri Differentiators

- **ECS architecture**: Bevy ECS provides deterministic frame-based state management, parallel system scheduling, and structural composition. No other WM uses ECS.
- **MCP server**: AI assistants can query window state, trigger layout changes, and automate workflows via kaname. No competitor has this.
- **Rhai scripting**: Type-safe, sandboxed scripting for layouts, rules, and status bar plugins. Safer than Lua (Hammerspoon), more powerful than shell (yabai).
- **GPU status bar**: Built-in SketchyBar-equivalent with ECS-reactive items, spring animation, and per-item scripting. Integrated, not a separate process.
- **Typed config**: shikumi YAML with Nix HM module generates validated config. Not free-form Lua or ad-hoc DSL.
- **Zero unsafe**: Pure safe Rust via objc2 bindings. No raw FFI, no C interop.
- **Testable by design**: 175+ unit tests, pure logic extraction, platform trait boundaries, deterministic FSM testing.

---

## Plugin System (soushi + Rhai)

Scripts are loaded from `~/.config/ayatsuri/scripts/*.rhai` at startup and hot-reloaded
via shikumi file watcher. Plugins extend ayatsuri in four ways:

### Layout Plugins

Custom tiling algorithms registered as named layouts:

```rhai
fn layout_info() {
    #{ name: "spiral", description: "Fibonacci spiral layout" }
}

fn layout(windows, screen) {
    // windows: array of #{ id, title, app }
    // screen: #{ x, y, width, height }
    // return: array of #{ id, x, y, width, height }
    let rects = [];
    let remaining = screen;
    for (i, win) in windows {
        if i == windows.len() - 1 {
            rects.push(#{ id: win.id, ..remaining });
        } else {
            let half = if i % 2 == 0 {
                #{ id: win.id, x: remaining.x, y: remaining.y,
                   width: remaining.width / 2, height: remaining.height }
            } else {
                #{ id: win.id, x: remaining.x, y: remaining.y,
                   width: remaining.width, height: remaining.height / 2 }
            };
            rects.push(half);
            // shrink remaining area
        }
    }
    rects
}
```

### Rule Plugins

Custom window matching and automatic actions:

```rhai
fn rule(window) {
    // window: #{ id, title, app, bundle_id }
    if window.bundle_id == "com.spotify.client" {
        return #{ space: 5, layout: "monocle" };
    }
    if window.title.contains("Picture in Picture") {
        return #{ floating: true, sticky: true };
    }
    ()  // no rule matched
}
```

### Status Bar Plugins

Custom bar items rendered via the status bar system:

```rhai
fn bar_item_info() {
    #{ id: "k8s_context", position: "right", update_freq: 10 }
}

fn bar_item_update() {
    let ctx = run("kubectl config current-context");
    #{ icon: "K8s", label: ctx.trim(), icon_color: "0xFF89b4fa" }
}
```

### Event Hooks

Scripts register callbacks for window manager events:
- `on_window_created(window)` — a new window appeared
- `on_window_destroyed(window)` — a window was closed
- `on_window_focused(window)` — focus changed
- `on_space_changed(from, to)` — workspace switch
- `on_display_changed(display)` — display added/removed/rearranged
- `on_layout_changed(layout_name)` — layout algorithm switched
- `on_config_reloaded()` — configuration was hot-reloaded

### Rhai API

| Function | Description |
|----------|-------------|
| `ayatsuri.windows()` | List all managed windows |
| `ayatsuri.focused()` | Get currently focused window |
| `ayatsuri.focus(id)` | Focus window by ID |
| `ayatsuri.close(id)` | Close window by ID |
| `ayatsuri.move(id, space)` | Move window to space |
| `ayatsuri.resize(id, rect)` | Resize window to rect |
| `ayatsuri.layout(name)` | Switch to named layout |
| `ayatsuri.space_create()` | Create new space |
| `ayatsuri.space_destroy(id)` | Destroy space |
| `ayatsuri.space_current()` | Get current space index |
| `ayatsuri.spaces()` | List all spaces |
| `ayatsuri.query(selector)` | Query windows by app/title pattern |
| `ayatsuri.rule_add(app, action)` | Add a window rule at runtime |
| `ayatsuri.rule_remove(app)` | Remove a window rule |
| `ayatsuri.displays()` | List all displays with bounds |
| `ayatsuri.config_get(key)` | Read config value |
| `ayatsuri.config_set(key, value)` | Set config value at runtime |
| `run(command)` | Execute shell command, return stdout |
| `notify(title, body)` | Send notification via tsuuchi |

---

## MCP Tools (kaname)

Ayatsuri embeds an MCP server via kaname (rmcp 0.15, stdio transport).

### Standard Tools

| Tool | Description |
|------|-------------|
| `status` | Server health, uptime, managed window count |
| `config_get` | Read current configuration |
| `config_set` | Update configuration at runtime |
| `version` | Binary version and build info |

### Window Management Tools

| Tool | Description |
|------|-------------|
| `list_windows` | List all managed windows with position, size, app, title |
| `focus_window` | Focus a window by ID |
| `move_window` | Move a window to coordinates or a space |
| `resize_window` | Resize a window to a rect |
| `close_window` | Close a window by ID |
| `get_focused` | Get the currently focused window |

### Space Management Tools

| Tool | Description |
|------|-------------|
| `list_spaces` | List all spaces with window counts |
| `create_space` | Create a new space |
| `switch_space` | Switch to a space by index |
| `move_to_space` | Move focused window to a space |

### Layout Tools

| Tool | Description |
|------|-------------|
| `set_layout` | Set the active layout algorithm |
| `get_layout` | Get the current layout name |
| `list_layouts` | List available layouts (built-in + plugin) |

### Display Tools

| Tool | Description |
|------|-------------|
| `list_displays` | List all displays with bounds and space assignments |

### Rule Tools

| Tool | Description |
|------|-------------|
| `set_rule` | Add or update a window rule |
| `list_rules` | List all active window rules |
| `remove_rule` | Remove a window rule |

### Status Bar Tools

| Tool | Description |
|------|-------------|
| `get_bar_state` | Get all bar item states |
| `set_bar_item` | Update a bar item's properties |
| `add_bar_item` | Add a new bar item at runtime |
| `remove_bar_item` | Remove a bar item |

### Config Tools

| Tool | Description |
|------|-------------|
| `reload_config` | Trigger config hot-reload |
| `get_config` | Read full configuration |
| `set_config` | Update a config key at runtime |
| `send_command` | Execute an arbitrary ayatsuri command by name |

---

## Configuration (shikumi)

Full YAML config schema at `~/.config/ayatsuri/ayatsuri.yaml`:

```yaml
# Layout defaults
layout:
  default: bsp             # bsp | monocle | columns | rows | stack | float | <plugin>
  gap: 8                   # gap between windows in pixels
  padding:                 # screen edge padding
    top: 36                # space for status bar
    bottom: 8
    left: 8
    right: 8

# Window rules (declarative)
rules:
  - app: "Firefox"
    space: 2
    layout: monocle
  - app: "Spotify"
    space: 5
    floating: false
  - title: "Picture in Picture"
    floating: true
    sticky: true
    layer: above
  - app: "Finder"
    floating: true
  - bundle_id: "com.apple.systempreferences"
    floating: true

# Keybindings (see Keybinding System section above for full spec)
bindings:
  window_focus_west: "cmd+h"
  window_focus_east: "cmd+l"
  window_focus_north: "cmd+k"
  window_focus_south: "cmd+j"
  window_swap_west: "cmd+shift+h"
  window_swap_east: "cmd+shift+l"
  layout_toggle: "cmd+shift+space"
  space_next: "ctrl+right"
  space_prev: "ctrl+left"
  quit: "ctrl+alt+q"
  reload_config: "ctrl+alt+r"

# Mode system
modes:
  resize:
    passthrough: false
    bindings:
      "h": window_shrink_west
      "l": window_grow_east
      "j": window_grow_south
      "k": window_shrink_north
      "=": window_equalize
      "escape": { mode: default }

# Status bar (see Status Bar section above for full spec)
status_bar:
  enabled: true
  position: top
  height: 28
  blur_radius: 20
  color: "0xCC1e1e2e"
  items:
    - id: spaces
      type: space
      position: left
    - id: front_app
      type: item
      position: left
      subscribe: [front_app_switched]
    - id: clock
      type: item
      position: right
      update_freq: 1
      script: "date '+%H:%M:%S'"

# Animation
animation:
  enabled: true
  stiffness: 800
  damping_ratio: 1.0
  epsilon: 0.5              # snap threshold in pixels

# Display settings
display:
  focus_follows_mouse: false
  mouse_follows_focus: false
  auto_balance: true        # equalize splits on window add/remove

# Overlay settings
overlay:
  dim_inactive: false
  dim_opacity: 0.85
  border_enabled: true
  border_width: 2
  border_color_active: "0xFFcba6f7"
  border_color_inactive: "0xFF313244"

# Scripting
scripting:
  enabled: true
  scripts_dir: "~/.config/ayatsuri/scripts"
  auto_reload: true

# MCP server
mcp:
  enabled: true
```
