{
  description = "Ayatsuri — programmable macOS automation framework";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    substrate = {
      url = "github:pleme-io/substrate";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    devenv = {
      url = "github:cachix/devenv";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      substrate,
      ...
    }:
    let
      system = "aarch64-darwin";
      pkgs = import nixpkgs { inherit system; };

      mkDate =
        longDate:
        (nixpkgs.lib.concatStringsSep "-" [
          (builtins.substring 0 4 longDate)
          (builtins.substring 4 2 longDate)
          (builtins.substring 6 2 longDate)
        ]);

      props = builtins.fromTOML (builtins.readFile ./Cargo.toml);
      version =
        props.package.version
        + "+date="
        + (mkDate (self.lastModifiedDate or "19700101"))
        + "_"
        + (self.shortRev or "dirty");

      pname = "ayatsuri";

      package = pkgs.rustPlatform.buildRustPackage {
        inherit pname version;
        src = pkgs.lib.cleanSource ./.;
        postPatch = ''
          substituteInPlace build.rs --replace-fail \
            'let sdk_dir = "/Library/Developer/CommandLineTools/SDKs";' \
            'let sdk_dir = "${pkgs.apple-sdk}/Platforms/MacOSX.platform/Developer/SDKs";'
        '';
        cargoLock.lockFile = ./Cargo.lock;
        buildInputs = [
          pkgs.apple-sdk.privateFrameworksHook
        ];

        # Do not run tests
        doCheck = false;

        meta = {
          mainProgram = pname;
        };
      };
    in
    {
      packages.${system} = {
        ayatsuri = package;
        default = package;
      };

      overlays.default = final: prev: {
        ayatsuri = self.packages.${final.system}.default;
      };

      homeManagerModules.default = import ./module {
        hmHelpers = import "${substrate}/lib/hm-service-helpers.nix" { lib = nixpkgs.lib; };
      };

      devShells.${system}.default = pkgs.mkShellNoCC {
        packages = [
          package
          pkgs.rustc
          pkgs.cargo
          pkgs.rust-analyzer
        ];
      };

      formatter.${system} = pkgs.nixfmt-tree;
    };
}
