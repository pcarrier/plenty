{
  description = "plenty - shared history for your fish shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crate2nix = {
      url = "github:nix-community/crate2nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, crate2nix }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };

        project = import ./Cargo.nix {
          inherit pkgs;
          buildRustCrateForPkgs = pkgs: pkgs.buildRustCrate.override {
            defaultCrateOverrides = pkgs.defaultCrateOverrides // {
              rusqlite = attrs: {
                buildInputs = (attrs.buildInputs or []) ++ [ pkgs.sqlite ];
              };
            };
          };
        };

        plenty = project.workspaceMembers.plenty.build;
        plentys = project.workspaceMembers.plentys.build;

      in
      {
        packages = {
          default = plenty;
          inherit plenty plentys;

          all = pkgs.symlinkJoin {
            name = "plenty-all";
            paths = [ plenty plentys ];
          };
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            cargo
            rustc
            rustfmt
            clippy
            rust-analyzer
            sqlite
            crate2nix.packages.${system}.default
          ];
        };
      }
    );
}
