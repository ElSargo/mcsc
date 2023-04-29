{
  description = "Dev shell the project";

  inputs = {
    fenix = {
      url = "github:nix-community/fenix/monthly";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nixpkgs.url = "nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs = { self, nixpkgs, flake-utils, fenix }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        cargoNix = pkgs.callPackage ./Cargo.nix {};
        rust = fenix.packages.${system}.complete.toolchain;
      in {
        cli = cargoNix.workspaceMembers.mcsc-cli.build ;
        server = cargoNix.workspaceMembers.mcsc-server.build ;
        # cli = cargoNix.workspaceMembers.mcsc-cli.build ;
        nixpkgs.overlays = [ fenix.overlays.complete ];
        devShells.default = pkgs.mkShell {
          buildInputs = [
            pkgs.protobuf
            rust
            pkgs.lldb_9
            pkgs.sccache
            pkgs.mold
            pkgs.clang
            pkgs.crate2nix
          ];
        };

        
      });
}

