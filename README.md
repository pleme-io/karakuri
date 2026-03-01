# Karakuri

A programmable macOS automation framework built on Bevy ECS.

## About

Karakuri is a macOS automation framework that extends sliding, tiling window
management with programmable scripting via Rhai, a Bevy plugin architecture,
and automation modules (clipboard, notifications, menu bar).

Forked from [Paneru](https://github.com/karinushka/paneru), Karakuri retains
the infinite-strip window management model while adding a full scripting layer.

Each monitor operates with its own independent window strip, ensuring that
windows remain confined to their respective displays and do not "overflow" onto
adjacent monitors.

## Features

- **Sliding tiling window management** — windows arranged on an infinite strip
- **Rhai scripting** — programmable hotkeys, event callbacks, and automation
- **Bevy plugin architecture** — modular, extensible design
- **Clipboard automation** — history, change detection, programmatic access
- **Notification system** — send notifications, handle click callbacks
- **Menu bar integration** — custom status bar items from scripts
- **Focus follows mouse** — optional mouse-driven focus
- **Touchpad gestures** — slide windows with swipe gestures
- **Hot-reload** — scripts and config reload without restart

## Installation

### Recommended System Options

- Karakuri requires accessibility access to move windows. Once it runs you may
  get a dialog window asking for permissions. Check System Settings under
  "Privacy & Security -> Accessibility".

- Check your System Settings for "Displays have separate spaces" option. It
  should be enabled — this allows Karakuri to manage workspaces independently.

- **Multiple displays**: Arrange additional displays above or below (not
  left/right) to prevent macOS from relocating off-screen windows.

### Installing with Nix

Add the karakuri flake to your inputs.

```nix
# flake.nix
inputs.karakuri = {
  url = "github:pleme-io/karakuri";
  inputs.nixpkgs.follows = "nixpkgs";
}
```

#### Home Manager

Karakuri provides a home manager module to install and configure it.

> [!NOTE]
> You still need to enable accessibility permissions in the macOS settings
> the first time karakuri is launched or any time it is updated.

```nix
# home.nix
{ inputs, ... }:

{
  imports = [
    inputs.karakuri.homeModules.karakuri
  ];

  services.karakuri = {
    enable = true;
    settings = {
      options = {
        preset_column_widths = [
          0.25
          0.33
          0.5
          0.66
          0.75
        ];
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
        window_swap_first = "alt + shift - h";
        window_swap_last = "alt + shift - l";
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

### Installing from source

```shell
$ git clone https://github.com/pleme-io/karakuri.git
$ cd karakuri
$ cargo build --release
$ cargo install --path .
```

### Configuration

Karakuri checks for configuration in the following locations:

- `$HOME/.karakuri`
- `$HOME/.karakuri.toml`
- `$XDG_CONFIG_HOME/karakuri/karakuri.toml`

Additionally it allows overriding the location with `$KARAKURI_CONFIG` environment variable.

Configuration changes are automatically reloaded while Karakuri is running.

### Running as a service

```shell
$ karakuri install
$ karakuri start
```

### Running in the foreground

```shell
$ karakuri
```

### Sending Commands

Karakuri exposes a `send-cmd` subcommand that lets you control the running
instance from the command line via a Unix socket (`/tmp/karakuri.socket`):

```shell
$ karakuri send-cmd <command> [args...]
```

#### Available commands

| Command                    | Description                                      |
| -------------------------- | ------------------------------------------------ |
| `window focus <direction>` | Move focus to a window in the given direction    |
| `window swap <direction>`  | Swap the focused window with a neighbour         |
| `window center`            | Center the focused window on screen              |
| `window resize`            | Cycle through `preset_column_widths`             |
| `window fullwidth`         | Toggle full-width mode for the focused window    |
| `window manage`            | Toggle managed/floating state                    |
| `window equalize`          | Distribute equal heights in the focused stack    |
| `window stack`             | Stack the focused window onto its left neighbour |
| `window unstack`           | Unstack the focused window into its own column   |
| `window nextdisplay`       | Move the focused window to the next display      |
| `mouse nextdisplay`        | Warp the mouse pointer to the next display       |
| `printstate`               | Print the internal ECS state to the debug log    |
| `quit`                     | Quit Karakuri                                    |

Where `<direction>` is one of: `west`, `east`, `north`, `south`, `first`, `last`.

## Architecture

Karakuri's architecture is built around the **Bevy ECS (Entity Component System)**,
which manages state as a collection of entities (displays, workspaces, applications,
and windows) and components.

The system is decoupled into three primary layers:

1.  **Platform Layer (`src/platform/`)**: Directly interfaces with macOS via `objc2` and Core Graphics.
2.  **Management Layer (`src/manager/`)**: Defines OS-agnostic traits that abstract window manipulation.
3.  **ECS Layer (`src/ecs/`)**: Bevy systems process incoming events, handle input triggers, and manage animations.

## Credits

Forked from [Paneru](https://github.com/karinushka/paneru) by Karinushka.
Window management inspired by [Yabai](https://github.com/koekeishiya/yabai),
[Niri](https://github.com/YaLTeR/niri), and [PaperWM.spoon](https://github.com/mogenson/PaperWM.spoon).

## License

MIT
