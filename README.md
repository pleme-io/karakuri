# Karakuri

A programmable macOS tiling window manager built on the Bevy ECS game engine. Karakuri arranges windows on a per-monitor infinite horizontal strip with support for vertical stacking, animated transitions, touchpad gestures, and full Rhai scripting. It also exposes an MCP server for live state inspection and command dispatch from Claude Code.

Forked from [Paneru](https://github.com/karinushka/paneru), Karakuri retains the infinite-strip layout model while adding a scripting layer, a plugin architecture, and an MCP integration.

## Architecture

Karakuri is structured as three decoupled layers connected by the Bevy ECS scheduler:

```
┌──────────────────────────────────────────────────────────────┐
│                     Bevy ECS Scheduler                       │
│                                                              │
│  PreUpdate     Event Ingestion (pump_events, triggers)       │
│  Update        State Transitions (window lifecycle, swipe)   │
│  PostUpdate    Layout → Animation → Rendering                │
└──────────┬─────────────────┬─────────────────┬───────────────┘
           │                 │                 │
    ┌──────▼──────┐   ┌─────▼──────┐   ┌──────▼──────┐
    │  Platform   │   │  Manager   │   │   Plugins   │
    │  (macOS)    │   │  (Traits)  │   │  (Bevy)     │
    │             │   │            │   │             │
    │  objc2      │   │  Window    │   │  Scripting  │
    │  CoreGraphx │   │  Display   │   │  Hotkey     │
    │  A11y API   │   │  Layout    │   │  Clipboard  │
    │  Gestures   │   │  Process   │   │  Overlay    │
    └─────────────┘   └────────────┘   │  MenuBar    │
                                       │  Snapshot   │
                                       │  Notify     │
                                       └─────────────┘
```

Every frame follows a strict five-stage pipeline. No system in a later stage may send events consumed by an earlier stage within the same frame. Layout computation is a pure function: given a window list, display bounds, config, and viewport offset, it emits reposition/resize markers without side effects.

An `InteractionMode` FSM (Idle, Dragging, Swiping, MissionControl) gates system execution via Bevy `States` run conditions.

## Features

- **Sliding tiling layout** -- windows arranged on an infinite horizontal strip per monitor
- **Vertical stacking** -- stack multiple windows in a single column with equal-height distribution
- **Rhai scripting** -- programmable hotkeys, event callbacks, and automation via `~/.config/karakuri/scripts/*.rhai`
- **Bevy plugin architecture** -- modular plugins for clipboard, notifications, menu bar, overlays, and snapshots
- **MCP server** -- live ECS state inspection and command dispatch for Claude Code (`karakuri mcp`)
- **Focus follows mouse** -- optional mouse-driven focus tracking
- **Touchpad gestures** -- four-finger swipe to scroll the window strip
- **Animated transitions** -- interpolated repositioning with instant-snap during guards and swipes
- **Window border overlays** -- colored borders and dim-inactive overlays via native Cocoa windows
- **Edge-snap drag preview** -- visual snap zones during window dragging
- **Hot-reload** -- configuration and scripts reload automatically without restart
- **Multi-display** -- independent window strips per monitor with directional focus/swap across displays
- **Fullscreen integration** -- navigate into and out of native macOS fullscreen windows
- **Shell command execution** -- bind arbitrary shell commands to hotkeys via `exec` bindings
- **Wallpaper control** -- set desktop wallpaper from config or scripts

## Installation

### Prerequisites

- macOS (Apple Silicon or Intel)
- Accessibility permissions (System Settings > Privacy & Security > Accessibility)
- "Displays have separate spaces" enabled in System Settings > Desktop & Dock

> **Multiple displays**: Arrange additional displays above or below (not left/right) to prevent macOS from relocating off-screen windows.

### Installing with Nix

```nix
# flake.nix
inputs.karakuri = {
  url = "github:pleme-io/karakuri";
  inputs.nixpkgs.follows = "nixpkgs";
};
```

#### Home Manager Module

```nix
{ inputs, ... }:
{
  imports = [ inputs.karakuri.homeManagerModules.default ];

  services.karakuri = {
    enable = true;
    settings = {
      options = {
        preset_column_widths = [ 0.25 0.33 0.5 0.66 0.75 ];
        swipe_gesture_fingers = 4;
        swipe_gesture_direction = "Natural";
        animation_speed = 4000;
      };
      bindings = {
        window_focus_west = "cmd - h";
        window_focus_east = "cmd - l";
        window_focus_north = "cmd - k";
        window_focus_south = "cmd - j";
        window_swap_west = "alt - h";
        window_swap_east = "alt - l";
        window_center = "alt - c";
        window_resize = "alt - r";
        window_fullwidth = "alt - f";
        window_manage = "ctrl + alt - t";
        window_stack = "alt - ]";
        window_unstack = "alt + shift - ]";
        quit = "ctrl + alt - q";
      };
    };
  };
}
```

### Installing from Source

```bash
git clone https://github.com/pleme-io/karakuri.git
cd karakuri
cargo build --release
cargo install --path .
```

## Usage

### Running as a Service

```bash
karakuri install    # Install launchd service
karakuri start      # Start the service
karakuri stop       # Stop the service
karakuri restart    # Restart the service
karakuri uninstall  # Remove the service
```

### Running in the Foreground

```bash
karakuri            # Launch directly (default subcommand)
```

### Sending Commands

Control the running daemon via Unix socket (`/tmp/karakuri.socket`):

```bash
karakuri send-cmd <command> [args...]
```

| Command                    | Description                                      |
| -------------------------- | ------------------------------------------------ |
| `window focus <direction>` | Move focus in the given direction                |
| `window swap <direction>`  | Swap the focused window with a neighbour         |
| `window center`            | Center the focused window on screen              |
| `window resize`            | Cycle through `preset_column_widths`             |
| `window fullwidth`         | Toggle full-width mode                           |
| `window manage`            | Toggle managed/floating state                    |
| `window equalize`          | Distribute equal heights in a stack              |
| `window stack`             | Stack onto the left neighbour                    |
| `window unstack`           | Unstack into its own column                      |
| `window nextdisplay`       | Move window to the next display                  |
| `mouse nextdisplay`        | Warp mouse to the next display                   |
| `printstate`               | Print ECS state to the debug log                 |
| `quit`                     | Quit Karakuri                                    |

Directions: `west`, `east`, `north`, `south`, `first`, `last`.

### MCP Server (Claude Code Integration)

```bash
karakuri mcp        # Start MCP server on stdio
```

Exposes five tools: `get_state`, `get_focused`, `get_displays`, `get_config`, and `send_command`.

## Configuration

Karakuri searches for configuration in order:

1. `$KARAKURI_CONFIG` environment variable
2. `$XDG_CONFIG_HOME/karakuri/karakuri.yaml` (preferred YAML format)
3. `$XDG_CONFIG_HOME/karakuri/karakuri.toml`
4. `$HOME/.karakuri.toml`
5. `$HOME/.karakuri`

Configuration changes are automatically hot-reloaded.

### Key Options

| Option | Type | Description |
|--------|------|-------------|
| `preset_column_widths` | `[f64]` | Width ratios to cycle through on resize |
| `animation_speed` | `u32` | Animation duration in milliseconds |
| `swipe_gesture_fingers` | `u8` | Number of fingers for swipe gestures |
| `swipe_gesture_direction` | `string` | `"Natural"` or `"Inverted"` |
| `focus_follows_mouse` | `bool` | Enable mouse-driven focus |
| `mouse_follows_focus` | `bool` | Warp mouse to focused window |
| `auto_center` | `bool` | Auto-center focused window |
| `edge_padding` | `[i32; 4]` | Top, right, bottom, left padding |
| `wallpaper` | `string` | Path to desktop wallpaper image |

## Development

```bash
cargo build          # Compile
cargo clippy         # Lint (pedantic warnings enabled)
cargo test           # Run unit tests (platform-independent, deterministic)
cargo run            # Launch (requires macOS Accessibility permissions)
```

Tests are platform-independent: `pump_events` is a no-op in test mode and events are injected via `world.write_message::<Event>()`. A static `TEST_MUTEX` serializes integration tests to prevent SIGABRT from parallel Bevy App initialization.

## Project Structure

| Path | Purpose |
|------|---------|
| `src/main.rs` | CLI entry point (clap), dispatches subcommands |
| `src/ecs/state.rs` | Bevy States enums, context resources, guards |
| `src/ecs/systems.rs` | Frame-driven systems (layout, animation, event pump) |
| `src/ecs/triggers.rs` | Observer-driven triggers (focus, workspace, config, drag) |
| `src/ecs/params.rs` | Custom SystemParams (Windows, ActiveDisplay, Configuration) |
| `src/ecs.rs` | Entity helpers, component/marker definitions, app setup |
| `src/commands.rs` | Command enum and all command handler systems |
| `src/config.rs` | TOML/YAML config parsing, keybinding resolution |
| `src/mcp.rs` | MCP server (stdio transport) for Claude Code |
| `src/plugins/window.rs` | WindowPlugin -- system registration and ordering |
| `src/plugins/scripting/` | Rhai scripting engine, API registration, script loader |
| `src/plugins/clipboard.rs` | Clipboard monitoring and history |
| `src/plugins/notification.rs` | macOS notification dispatch |
| `src/plugins/menu_bar.rs` | Status bar item management |
| `src/plugins/hotkey.rs` | Global hotkey registration |
| `src/plugins/snapshot.rs` | State snapshot for MCP queries |
| `src/overlay.rs` | Window border and dim-inactive overlay rendering |
| `src/manager/` | Window, Display, LayoutStrip, Process abstractions |
| `src/platform/` | macOS platform layer (Accessibility API, gestures, service) |
| `module/` | Nix home-manager module |

## Related Projects

- [substrate](https://github.com/pleme-io/substrate) -- Nix build patterns (provides `hm-service-helpers`)
- [blackmatter](https://github.com/pleme-io/blackmatter) -- Home-manager module aggregator
- [Paneru](https://github.com/karinushka/paneru) -- Original upstream project

## Credits

Forked from [Paneru](https://github.com/karinushka/paneru) by Karinushka. Window management inspired by [Yabai](https://github.com/koekeishiya/yabai), [Niri](https://github.com/YaLTeR/niri), and [PaperWM.spoon](https://github.com/mogenson/PaperWM.spoon).

## License

[MIT](LICENSE.txt)
