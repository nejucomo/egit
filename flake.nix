{
  description = "egit — interactive visual git history explorer";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    flake-utils.url = "github:numtide/flake-utils";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, crane }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        # Native build tools (available at build time)
        nativeBuildInputs = with pkgs; [
          pkg-config
        ];

        # Runtime / link-time libraries required by eframe/egui
        buildInputs = with pkgs; [
          libGL
          libxkbcommon
          wayland
          xorg.libX11
          xorg.libXcursor
          xorg.libXi
          xorg.libXrandr
        ];

        egit = craneLib.buildPackage {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;
          inherit nativeBuildInputs buildInputs;
        };
      in
      {
        packages = {
          default = egit;
          inherit egit;
        };

        devShells.default = craneLib.devShell {
          # Inherit inputs from the package build so the shell can build egit
          inputsFrom = [ egit ];

          # Extra tools useful during development
          packages = with pkgs; [
            git
            rust-analyzer
          ];
        };
      });
}
