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

  # Merge systemDefaults into settings as the `system_defaults` key
  effectiveSettings =
    if cfg.settings == null then null
    else if cfg.systemDefaults == { } then cfg.settings
    else cfg.settings // { system_defaults = cfg.systemDefaults; };

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

    # Launchd agent
    (mkLaunchdService {
      name = "ayatsuri";
      label = "io.pleme.ayatsuri";
      command = "${cfg.package}/bin/ayatsuri";
      args = [ "launch" ];
      logDir = logDir;
      processType = "Interactive";
      keepAlive = true;
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
