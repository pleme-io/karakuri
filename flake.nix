{
  description = "Ayatsuri — programmable macOS automation framework";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    crate2nix.url = "github:nix-community/crate2nix";
    flake-utils.url = "github:numtide/flake-utils";
    substrate = {
      url = "github:pleme-io/substrate";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    crate2nix,
    flake-utils,
    substrate,
    ...
  }: let
    # Ayatsuri is macOS-only (uses Accessibility API, CGEventTap, etc.)
    systems = [ "aarch64-darwin" "x86_64-darwin" ];

    mkPerSystem = system: let
      pkgs = import nixpkgs { inherit system; };
      darwinHelpers = import "${substrate}/lib/darwin.nix";

      project = import ./Cargo.nix {
        inherit pkgs;
        defaultCrateOverrides = pkgs.defaultCrateOverrides // {
          ayatsuri = attrs: {
            buildInputs = (attrs.buildInputs or [])
              ++ [ pkgs.apple-sdk.privateFrameworksHook ]
              ++ (darwinHelpers.mkDarwinBuildInputs pkgs);
            postPatch = ''
              substituteInPlace build.rs --replace-fail \
                'let sdk_dir = "/Library/Developer/CommandLineTools/SDKs";' \
                'let sdk_dir = "${pkgs.apple-sdk}/Platforms/MacOSX.platform/Developer/SDKs";'
            '';
          };
        };
      };

      package = project.rootCrate.build;
    in {
      packages = {
        default = package;
        ayatsuri = package;
      };

      devShells.default = pkgs.mkShellNoCC {
        packages = [
          pkgs.rustc
          pkgs.cargo
          pkgs.rust-analyzer
          crate2nix.packages.${system}.default
        ] ++ [ pkgs.apple-sdk.privateFrameworksHook ]
          ++ (darwinHelpers.mkDarwinBuildInputs pkgs);
      };

      apps.default = {
        type = "app";
        program = "${package}/bin/ayatsuri";
      };
    };

    flakeWrapper = import "${substrate}/lib/flake-wrapper.nix" { inherit nixpkgs; };
  in
    flakeWrapper.mkFlakeOutputs {
      inherit systems mkPerSystem;
      extraOutputs = {
        overlays.default = final: prev: {
          ayatsuri = (mkPerSystem final.system).packages.default;
        };
        homeManagerModules.default = import ./module {
          hmHelpers = import "${substrate}/lib/hm-service-helpers.nix" { lib = nixpkgs.lib; };
        };
      };
    };
}
