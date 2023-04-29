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
      in {
        defaultPackage = naersk'.buildPackage {
          src = ./.;
          nativeBuildInputs = [ pkgs.protobuf ];
        };
        buildInputs = [ pkgs.protobuf ];
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

