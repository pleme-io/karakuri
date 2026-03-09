# Module factory — receives { hmHelpers } from flake.nix
{ hmHelpers }:
{
  lib,
  config,
  pkgs,
  ...
}:
with lib;
let
  inherit (hmHelpers) mkLaunchdService;
  cfg = config.blackmatter.components.ayatsuri;
  isDarwin = pkgs.stdenv.isDarwin;

  # Build the status_bar config section from typed options.
  statusBarSettings = let sb = cfg.statusBar; in
    optionalAttrs sb.enable ({
      enabled = true;
      position = sb.position;
      height = sb.height;
      blur_radius = sb.blurRadius;
      color = sb.color;
      border_color = sb.borderColor;
      border_width = sb.borderWidth;
      corner_radius = sb.cornerRadius;
      font = sb.font;
      icon_font = sb.iconFont;
      padding_left = sb.paddingLeft;
      padding_right = sb.paddingRight;
      y_offset = sb.yOffset;
      topmost = sb.topmost;
      sticky = sb.sticky;
      display = sb.display;
      notch_width = sb.notchWidth;
      hide_macos_menubar = sb.hideMacosMenubar;
      defaults = {
        icon_color = sb.defaults.iconColor;
        label_color = sb.defaults.labelColor;
        background_color = sb.defaults.backgroundColor;
        padding_left = sb.defaults.paddingLeft;
        padding_right = sb.defaults.paddingRight;
      };
      items = map (item: filterAttrs (_: v: v != null) {
        id = item.id;
        type = item.type;
        position = item.position;
        icon = item.icon;
        label = item.label;
        script = item.script;
        update_freq = item.updateFreq;
        subscribe = if item.subscribe == [ ] then null else item.subscribe;
        icon_color = item.iconColor;
        label_color = item.labelColor;
        background_color = item.backgroundColor;
        background_corner_radius = item.backgroundCornerRadius;
        padding_left = item.paddingLeft;
        padding_right = item.paddingRight;
      }) sb.items;
    });

  # Merge systemDefaults and statusBar into settings
  effectiveSettings =
    if cfg.settings == null then null
    else let
      withDefaults = if cfg.systemDefaults == { } then cfg.settings
        else cfg.settings // { system_defaults = cfg.systemDefaults; };
      withBar = if cfg.statusBar.enable then withDefaults // { status_bar = statusBarSettings; }
        else withDefaults;
    in withBar;

  # Generate YAML config from nix attrs (following kindling pattern)
  yamlConfig = pkgs.writeText "ayatsuri.yaml"
    (lib.generators.toYAML { } effectiveSettings);

  logDir =
    if isDarwin then "${config.home.homeDirectory}/Library/Logs" else "${config.home.homeDirectory}/.local/share/ayatsuri/logs";
in
{
  options.blackmatter.components.ayatsuri = {
    enable = mkEnableOption "Ayatsuri — programmable macOS automation framework";

    package = mkOption {
      type = types.package;
      default = pkgs.ayatsuri;
      description = "The ayatsuri package to use.";
    };

    settings = mkOption {
      type = types.nullOr types.attrs;
      default = null;
      description = ''
        Configuration written to `~/.config/ayatsuri/ayatsuri.yaml`.
        Accepts any attrs that serialize to valid ayatsuri YAML config.
        Figment loads: defaults → env vars (AYATSURI_*) → this file.
      '';
      example = {
        options = {
          focus_follows_mouse = true;
          preset_column_widths = [
            0.25
            0.33
            0.5
            0.66
            0.75
          ];
          swipe_gesture_fingers = 4;
          animation_speed = 4000;
        };
        bindings = {
          window_focus_west = "cmd - h";
          window_focus_east = "cmd - l";
          window_focus_north = "cmd - k";
          window_focus_south = "cmd - j";
          quit = "ctrl + alt - q";
        };
        windows = {
          pip = {
            title = "picture.*picture";
            bundle_id = "com.something.apple";
            floating = true;
            index = 1;
          };
        };
        scripting = {
          init_script = "~/.config/ayatsuri/init.rhai";
          script_dirs = [ "~/.config/ayatsuri/scripts" ];
          hot_reload = true;
        };
      };
    };

    systemDefaults = mkOption {
      type = types.attrsOf (types.attrsOf types.anything);
      default = { };
      description = ''
        macOS defaults applied by ayatsuri at startup and hot-reload.
        Outer key = domain (e.g. "com.apple.dock"), inner key = preference key.
        Merged into `system_defaults` in the generated YAML config.
      '';
      example = {
        "com.apple.dock" = {
          autohide = true;
          autohide-delay = 0.0;
        };
      };
    };

    statusBar = {
      enable = mkEnableOption "Built-in status bar (SketchyBar-style)";

      position = mkOption {
        type = types.enum [ "top" "bottom" ];
        default = "top";
        description = "Bar position: top or bottom of screen.";
      };

      height = mkOption {
        type = types.int;
        default = 28;
        description = "Bar height in points.";
      };

      blurRadius = mkOption {
        type = types.int;
        default = 20;
        description = "Background blur radius (0 = no blur, solid color only).";
      };

      color = mkOption {
        type = types.str;
        default = "0xCC1e1e2e";
        description = "Background color in ARGB hex (0xAARRGGBB or #RRGGBB).";
      };

      borderColor = mkOption {
        type = types.str;
        default = "0xFF313244";
        description = "Border color in ARGB hex.";
      };

      borderWidth = mkOption {
        type = types.float;
        default = 0.0;
        description = "Border width in points.";
      };

      cornerRadius = mkOption {
        type = types.float;
        default = 0.0;
        description = "Corner radius for the bar itself.";
      };

      font = mkOption {
        type = types.str;
        default = "Hack Nerd Font:Regular:14.0";
        description = "Default font spec: Family:Style:Size.";
      };

      iconFont = mkOption {
        type = types.str;
        default = "Hack Nerd Font:Regular:16.0";
        description = "Default icon font spec: Family:Style:Size.";
      };

      paddingLeft = mkOption {
        type = types.float;
        default = 8.0;
        description = "Left padding in points.";
      };

      paddingRight = mkOption {
        type = types.float;
        default = 8.0;
        description = "Right padding in points.";
      };

      yOffset = mkOption {
        type = types.float;
        default = 0.0;
        description = "Vertical offset in points.";
      };

      topmost = mkOption {
        type = types.bool;
        default = false;
        description = "Whether the bar is above all windows.";
      };

      sticky = mkOption {
        type = types.bool;
        default = true;
        description = "Whether the bar is visible on all spaces.";
      };

      display = mkOption {
        type = types.str;
        default = "all";
        description = "Which displays to show on: all, main, or display index.";
      };

      notchWidth = mkOption {
        type = types.int;
        default = 220;
        description = "Space reserved for MacBook notch (points).";
      };

      hideMacosMenubar = mkOption {
        type = types.bool;
        default = false;
        description = "Auto-hide the native macOS menu bar when status bar is active.";
      };

      defaults = {
        iconColor = mkOption {
          type = types.str;
          default = "0xFFcdd6f4";
          description = "Default icon color for all items (ARGB hex).";
        };

        labelColor = mkOption {
          type = types.str;
          default = "0xFFcdd6f4";
          description = "Default label color for all items (ARGB hex).";
        };

        backgroundColor = mkOption {
          type = types.str;
          default = "0x00000000";
          description = "Default background color for all items (ARGB hex).";
        };

        paddingLeft = mkOption {
          type = types.float;
          default = 6.0;
          description = "Default left padding for items.";
        };

        paddingRight = mkOption {
          type = types.float;
          default = 6.0;
          description = "Default right padding for items.";
        };
      };

      items = mkOption {
        type = types.listOf (types.submodule {
          options = {
            id = mkOption {
              type = types.str;
              description = "Unique item identifier (e.g., clock, battery, spaces.1).";
            };

            type = mkOption {
              type = types.enum [ "item" "space" "bracket" "graph" "slider" "alias" ];
              default = "item";
              description = "Item type.";
            };

            position = mkOption {
              type = types.enum [ "left" "right" "center" "q" "e" ];
              default = "left";
              description = "Position on the bar.";
            };

            icon = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = "Icon text (glyph, emoji, Nerd Font symbol).";
            };

            label = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = "Static label text.";
            };

            script = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = "Shell script for dynamic label updates.";
            };

            updateFreq = mkOption {
              type = types.int;
              default = 0;
              description = "Update frequency in seconds (0 = event-driven only).";
            };

            subscribe = mkOption {
              type = types.listOf types.str;
              default = [ ];
              description = "Events this item subscribes to.";
            };

            iconColor = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = "Icon color override (ARGB hex).";
            };

            labelColor = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = "Label color override (ARGB hex).";
            };

            backgroundColor = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = "Background color override (ARGB hex).";
            };

            backgroundCornerRadius = mkOption {
              type = types.nullOr types.float;
              default = null;
              description = "Background corner radius.";
            };

            paddingLeft = mkOption {
              type = types.nullOr types.float;
              default = null;
              description = "Left padding override.";
            };

            paddingRight = mkOption {
              type = types.nullOr types.float;
              default = null;
              description = "Right padding override.";
            };
          };
        });
        default = [ ];
        description = ''
          Bar items (widgets). Each item is a widget on the bar.
          Built-in items (clock, front_app) are auto-added if not defined.
          Items with scripts are updated periodically or on subscribed events.
        '';
        example = [
          {
            id = "front_app";
            position = "left";
            icon = "";
            subscribe = [ "front_app_switched" ];
          }
          {
            id = "clock";
            position = "right";
            updateFreq = 30;
            script = "date '+%H:%M'";
          }
          {
            id = "battery";
            position = "right";
            icon = "";
            script = "pmset -g batt | grep -Eo '[0-9]+%'";
            updateFreq = 120;
          }
        ];
      };
    };

    scripting = {
      initScript = mkOption {
        type = types.lines;
        default = "";
        description = ''
          Contents of `~/.config/ayatsuri/init.rhai`.
          Main Rhai script loaded on startup.
        '';
        example = ''
          log("ayatsuri init.rhai loaded");
          on_hotkey("cmd-h", || focus_west());
        '';
      };

      extraScripts = mkOption {
        type = types.attrsOf types.lines;
        default = { };
        description = ''
          Additional Rhai scripts written to `~/.config/ayatsuri/scripts/<name>.rhai`.
        '';
        example = {
          "window-rules" = ''
            log("window rules loaded");
          '';
        };
      };

      hotReload = mkOption {
        type = types.bool;
        default = true;
        description = "Enable hot-reload of Rhai scripts on file changes.";
      };
    };
  };

  config = mkIf (cfg.enable && isDarwin) (mkMerge [
    # Install the package
    {
      home.packages = [ cfg.package ];
    }

    # Create log directory
    {
      home.activation.ayatsuri-log-dir = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
        run mkdir -p "${logDir}"
      '';
    }

    # Launchd agent — restart only on real crashes, not permission errors
    (mkLaunchdService {
      name = "ayatsuri";
      label = "io.pleme.ayatsuri";
      command = "${cfg.package}/bin/ayatsuri";
      args = [ "launch" ];
      logDir = logDir;
      processType = "Interactive";
      keepAlive = {
        Crashed = true;
        SuccessfulExit = false;
      };
    })

    # YAML configuration (figment-based, hot-reloaded)
    (mkIf (cfg.settings != null) {
      xdg.configFile."ayatsuri/ayatsuri.yaml".source = yamlConfig;
    })

    # Rhai init script
    (mkIf (cfg.scripting.initScript != "") {
      xdg.configFile."ayatsuri/init.rhai".text = cfg.scripting.initScript;
    })

    # Extra Rhai scripts
    (mkIf (cfg.scripting.extraScripts != { }) {
      xdg.configFile = mapAttrs' (name: content:
        nameValuePair "ayatsuri/scripts/${name}.rhai" { text = content; }
      ) cfg.scripting.extraScripts;
    })
  ]);
}
