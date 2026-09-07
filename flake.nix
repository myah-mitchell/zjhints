{
  description = "Keybinding hints plugin for zjstatus";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    flake-utils,
    crane,
    rust-overlay,
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [(import rust-overlay)];
      };

      # `stable.latest` only resolves as new as whatever the locked
      # rust-overlay revision knew about — it does not track "actually
      # latest". If flake.lock goes stale relative to Cargo.toml's
      # rust-version, this quietly hands cargo a toolchain too old to satisfy
      # its own MSRV, and the build fails deep in cargo rather than here.
      # Assert it instead, so the failure names the real cause and the fix.
      cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
      msrv = cargoToml.package.rust-version;

      rustWithWasiTarget = let
        toolchain = pkgs.rust-bin.stable.latest.default.override {
          targets = ["wasm32-wasip1"];
        };
      in
        assert pkgs.lib.assertMsg (builtins.compareVersions toolchain.version msrv >= 0) ''
          rust-overlay resolves rustc ${toolchain.version} for `stable.latest`,
          which is older than Cargo.toml's rust-version = "${msrv}".
          flake.lock's rust-overlay/nixpkgs inputs are stale relative to
          Cargo.toml. Fix with:
            nix flake update rust-overlay nixpkgs
        '';
        toolchain;

      craneLib = (crane.mkLib pkgs).overrideToolchain rustWithWasiTarget;

      zjhints = craneLib.buildPackage {
        src = craneLib.cleanCargoSource (craneLib.path ./.);
        cargoExtraArgs = "--target wasm32-wasip1";
        doCheck = false;
        doNotSign = true;
      };
    in {
      packages.default = zjhints;

      devShells.default = craneLib.devShell {
        packages = with pkgs; [
          rustWithWasiTarget
          wasmtime
        ];
      };
    });
}
