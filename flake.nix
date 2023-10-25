{
  description = "A controler for mincraft servers.";

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
        aarch_64_pkgsCross = import nixpkgs {
          inherit system;
          crossSystem = { config = "aarch64-unknown-linux-gnu"; };
        };
        aarch_64_naersk_cross = aarch_64_pkgsCross.callPackage naersk { };
      in {
        defaultPackage = naersk'.buildPackage {
          src = ./.;
          buildInputs = with pkgs; [ gcc cmake glibc stdenv.cc ];
        };

        packages.aarch64-unknown-linux-gnu = aarch_64_naersk_cross.buildPackage {
          src = ./.;
          nativeBuildInputs = with aarch_64_pkgsCross; [ gcc cmake glibc stdenv.cc ];
          buildInputs = with aarch_64_pkgsCross; [ gcc cmake glibc stdenv.cc ];
        };

        nixpkgs.overlays = [ fenix.overlays.complete ];

        devShells.default = pkgs.mkShell {
          buildInputs = [
            rust
            pkgs.lldb_9
            pkgs.sccache
            pkgs.mold
            pkgs.clang
          ];
        };

      });
}

