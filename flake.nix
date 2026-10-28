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
  };

  outputs = { nixpkgs, rust-overlay, devshell, flake-utils, ... }: 
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ (import rust-overlay) devshell.overlays.default ];
      };

      toolchain_fn = p: p.rust-bin.selectLatestNightlyWith (toolchain: toolchain.default.override {
        extensions = [ "rust-src" "rust-analyzer" ];
      });

      extra_utils = with pkgs; [ gcc bacon cargo-nextest cargo-fuzz ];
    in {
      devShells.default = pkgs.devshell.mkShell {
        packages = [ (toolchain_fn pkgs) ] ++ extra_utils;
        motd = "\n  Welcome to the {2}$(basename $PRJ_ROOT){reset} shell.\n";
      };
    });
}
