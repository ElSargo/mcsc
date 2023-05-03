{
  description = "Dev shell the project";

  inputs = {
    naersk.url = "github:nix-community/naersk";
    fenix = {
      url = "github:nix-community/fenix/monthly";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nixpkgs.url = "nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils, naersk, fenix }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        rust = fenix.packages.${system}.complete.toolchain;
        naersk' = pkgs.callPackage naersk { };
        pkgs_cross = import nixpkgs {
          inherit system;
          crossSystem = { config = "aarch64-unknown-linux-gnu"; };
        };
        naersk_cross = pkgs_cross.callPackage naersk { };
      in {
        defaultPackage = naersk'.buildPackage {
          src = ./.;
          nativeBuildInputs = with pkgs; [ protobuf ];
          buildInputs = with pkgs; [ gcc cmake glibc stdenv.cc ];
        };

        packages.aarch64-unknown-linux-gnu = naersk_cross.buildPackage {
          src = ./.;
          nativeBuildInputs = [ pkgs.protobuf pkgs_cross.gcc pkgs_cross.cmake pkgs_cross.glibc pkgs_cross.stdenv.cc ];
          buildInputs = with pkgs_cross; [ gcc cmake glibc stdenv.cc ];
        };

        nixpkgs.overlays = [ fenix.overlays.complete ];

        devShells.default = pkgs.mkShell {
          buildInputs = [
            pkgs.protobuf
            rust
            pkgs.lldb_9
            pkgs.sccache
            pkgs.mold
            pkgs.clang
          ];
        };

      });
}

