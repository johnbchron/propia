{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    devshell = {
      url = "github:numtide/devshell";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
  };

  outputs = { nixpkgs, rust-overlay, devshell, flake-utils, crane, ... }: 
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ (import rust-overlay) devshell.overlays.default ];
      };

      inherit (pkgs) lib;

      toolchain_fn = p: p.rust-bin.selectLatestNightlyWith (toolchain: toolchain.default.override {
        extensions = [ "rust-src" "rust-analyzer" ];
      });

      craneLib = (crane.mkLib pkgs).overrideToolchain toolchain_fn;
      src = lib.fileset.toSource {
        root = ./.;
        fileset = lib.fileset.unions [
          (craneLib.fileset.commonCargoSources ./.)
          ./corpora
        ];
      };

      pname = "propia";
      version = "0.1";

      commonArgs = {
        inherit src pname version;
        strictDeps = true;
      };

      cargoArtifacts = craneLib.buildDepsOnly commonArgs;

      extra_utils = with pkgs; [ gcc bacon cargo-nextest cargo-fuzz ];
    in {
      devShells.default = pkgs.devshell.mkShell {
        packages = [ (toolchain_fn pkgs) ] ++ extra_utils;
        motd = "\n  Welcome to the {2}$(basename $PRJ_ROOT){reset} shell.\n";
      };

      checks = {
        propia-clippy = craneLib.cargoClippy (commonArgs // {
          inherit cargoArtifacts;
          cargoClippyExtraArgs = "--all-targets -- --deny warnings";
        });
        propia-doc = craneLib.cargoDoc (commonArgs // {
          inherit cargoArtifacts;
          env.RUSTDOCFLAGS = "--deny warnings";
        });
        propia-fmt = craneLib.cargoFmt { inherit src pname version; };
        propia-nextest = craneLib.cargoNextest (commonArgs // {
          inherit cargoArtifacts;
          partitions = 1;
          partitionType = "count";
          cargoNextestPartitionsExtraArgs = "--no-tests=pass";
        });
      };
    });
}
