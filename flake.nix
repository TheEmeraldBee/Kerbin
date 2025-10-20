{
  description = "Kerbin - The Space-Age Text Editor";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        # Main Kerbin package - standard Nix build
        kerbin = pkgs.rustPlatform.buildRustPackage {
          pname = "kerbin";
          version = "0.1.0";
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = with pkgs; [
            pkg-config
          ];

          buildInputs = with pkgs; [
            # Add any required libraries here
          ];

          # Patch the config path to use XDG_CONFIG_HOME or default
          postPatch = ''
            substituteInPlace Cargo.toml \
              --replace 'config = { path = "../config" }' \
                        'config = { path = "''${XDG_CONFIG_HOME:-$HOME/.config}/kerbin" }'
          '';

          meta = with pkgs.lib; {
            description = "The Space-Age Text Editor";
            homepage = "https://github.com/TheEmeraldBee/Kerbin";
            license = licenses.mit;
            maintainers = [ ];
            mainProgram = "kerbin";
          };
        };

        # Installer wrapper that uses the install.sh script
        # This allows users to build from source with caching
        kerbin-installer = pkgs.stdenv.mkDerivation {
          pname = "kerbin-installer";
          version = "0.1.0";
          src = ./.;

          nativeBuildInputs = [ pkgs.makeWrapper ];

          # Don't build anything, just install the script
          dontBuild = true;
          dontConfigure = true;

          installPhase = ''
            mkdir -p $out/bin
            mkdir -p $out/share/kerbin

            # Copy the install script
            cp install.sh $out/share/kerbin/install.sh
            chmod +x $out/share/kerbin/install.sh

            # Create a wrapper that ensures the right tools are available
            makeWrapper $out/share/kerbin/install.sh $out/bin/kerbin-install \
              --prefix PATH : ${
                pkgs.lib.makeBinPath [
                  pkgs.cargo
                  pkgs.rustc
                  pkgs.git
                  pkgs.coreutils
                  pkgs.gnused
                ]
              } \
              --set KERBIN_NIX_BUILD "true"
          '';

          meta = with pkgs.lib; {
            description = "Installation script for Kerbin editor";
            homepage = "https://github.com/TheEmeraldBee/Kerbin";
            license = licenses.mit;
            mainProgram = "kerbin-install";
          };
        };

        # Complete package with both binary and installer
        kerbin-full = pkgs.symlinkJoin {
          name = "kerbin-full";
          paths = [
            kerbin
            kerbin-installer
          ];
          meta = kerbin.meta // {
            description = "Kerbin editor with installation tools";
          };
        };

      in
      {
        packages = {
          default = kerbin;
          kerbin = kerbin;
          installer = kerbin-installer;
          full = kerbin-full;
        };

        apps = {
          default = {
            type = "app";
            program = "${kerbin}/bin/kerbin";
          };
          kerbin = {
            type = "app";
            program = "${kerbin}/bin/kerbin";
          };
          install = {
            type = "app";
            program = "${kerbin-installer}/bin/kerbin-install";
          };
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            cargo
            rustc
            rust-analyzer
            clippy
            rustfmt
            git
            pkg-config
          ];

          shellHook = ''
            echo "🚀 Kerbin Development Environment"
            echo "================================="
            echo ""
            echo "Available commands:"
            echo "  cargo build --release    - Build the editor"
            echo "  cargo run                - Run the editor directly"
            echo "  ./install.sh             - Install with interactive setup"
            echo "  ./install.sh --rebuild   - Rebuild with saved settings"
            echo "  ./install.sh -r -y       - Fast rebuild without prompts"
            echo ""
            echo "Nix flake commands:"
            echo "  nix build                - Build kerbin package"
            echo "  nix run                  - Run kerbin directly"
            echo "  nix run .#install        - Run the installer"
            echo "  nix build .#full         - Build with installer included"
          '';
        };
      }
    );
}
