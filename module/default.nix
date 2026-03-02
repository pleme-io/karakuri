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
  cfg = config.blackmatter.components.karakuri;
  isDarwin = pkgs.stdenv.isDarwin;

  # Generate YAML config from nix attrs (following kindling pattern)
  yamlConfig = pkgs.writeText "karakuri.yaml"
    (lib.generators.toYAML { } cfg.settings);

  logDir =
    if isDarwin then "${config.home.homeDirectory}/Library/Logs" else "${config.home.homeDirectory}/.local/share/karakuri/logs";
in
{
  options.blackmatter.components.karakuri = {
    enable = mkEnableOption "Karakuri — programmable macOS automation framework";

    package = mkOption {
      type = types.package;
      default = pkgs.karakuri;
      description = "The karakuri package to use.";
    };

    settings = mkOption {
      type = types.nullOr types.attrs;
      default = null;
      description = ''
        Configuration written to `~/.config/karakuri/karakuri.yaml`.
        Accepts any attrs that serialize to valid karakuri YAML config.
        Figment loads: defaults → env vars (KARAKURI_*) → this file.
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
          init_script = "~/.config/karakuri/init.rhai";
          script_dirs = [ "~/.config/karakuri/scripts" ];
          hot_reload = true;
        };
      };
    };

    scripting = {
      initScript = mkOption {
        type = types.lines;
        default = "";
        description = ''
          Contents of `~/.config/karakuri/init.rhai`.
          Main Rhai script loaded on startup.
        '';
        example = ''
          log("karakuri init.rhai loaded");
          on_hotkey("cmd-h", || focus_west());
        '';
      };

      extraScripts = mkOption {
        type = types.attrsOf types.lines;
        default = { };
        description = ''
          Additional Rhai scripts written to `~/.config/karakuri/scripts/<name>.rhai`.
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
      home.activation.karakuri-log-dir = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
        run mkdir -p "${logDir}"
      '';
    }

    # Launchd agent
    (mkLaunchdService {
      name = "karakuri";
      label = "io.pleme.karakuri";
      command = "${cfg.package}/bin/karakuri";
      args = [ "launch" ];
      logDir = logDir;
      processType = "Interactive";
      keepAlive = true;
    })

    # YAML configuration (figment-based, hot-reloaded)
    (mkIf (cfg.settings != null) {
      xdg.configFile."karakuri/karakuri.yaml".source = yamlConfig;
    })

    # Rhai init script
    (mkIf (cfg.scripting.initScript != "") {
      xdg.configFile."karakuri/init.rhai".text = cfg.scripting.initScript;
    })

    # Extra Rhai scripts
    (mkIf (cfg.scripting.extraScripts != { }) {
      xdg.configFile = mapAttrs' (name: content:
        nameValuePair "karakuri/scripts/${name}.rhai" { text = content; }
      ) cfg.scripting.extraScripts;
    })
  ]);
}
