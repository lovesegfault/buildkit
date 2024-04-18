{
  inputs = {
    cachix = {
      url = "github:cachix/cachix";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        devenv.follows = "devenv";
        flake-compat.follows = "flake-compat";
        pre-commit-hooks.follows = "pre-commit-hooks";
      };
    };
    devenv = {
      url = "github:cachix/devenv";
      inputs = {
        pre-commit-hooks.follows = "pre-commit-hooks";
        nixpkgs.follows = "nixpkgs";
        flake-compat.follows = "flake-compat";
        cachix.follows = "cachix";
      };
    };
    flake-compat = {
      url = "github:edolstra/flake-compat";
      flake = false;
    };
    flake-utils = {
      url = "github:numtide/flake-utils";
    };
    flake-parts.url = "github:hercules-ci/flake-parts";
    gitignore = {
      url = "github:hercules-ci/gitignore.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nci = {
      url = "github:yusdacra/nix-cargo-integration";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        parts.follows = "flake-parts";
        treefmt.follows = "treefmt";
      };
    };
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    pre-commit-hooks = {
      url = "github:cachix/pre-commit-hooks.nix";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-compat.follows = "flake-compat";
        flake-utils.follows = "flake-utils";
        gitignore.follows = "gitignore";
      };
    };
    treefmt = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        inputs.nci.flakeModule
        inputs.pre-commit-hooks.flakeModule
        inputs.treefmt.flakeModule
      ];
      systems = [ "aarch64-linux" "aarch64-darwin" "x86_64-linux" ];
      perSystem = { config, self', inputs', lib, pkgs, system, ... }:
        let
          inherit (config.nci.outputs) buildkit;
        in
        {
          nci = {
            projects.buildkit.path = ./.;
            crates.buildkit = { };
          };

          packages.default = buildkit.packages.release;

          devShells.default = buildkit.devShell.overrideAttrs (old: {
            nativeBuildInputs = (old.nativebuildInputs or [ ])
              ++ (with config.treefmt.build; [ wrapper ] ++ (lib.attrValues programs))
              ++ (with config.pre-commit.settings; [ package ] ++ enabledPackages)
              ++ (with pkgs; [ nil rust-analyzer ])
            ;
            shellHook = (old.shellHook or "") + config.pre-commit.installationScript;
          });

          pre-commit = {
            check.enable = true;
            settings.hooks = {
              mdl.enable = true;
              statix.enable = true;
              treefmt.enable = true;
            };
          };
          treefmt = {
            projectRootFile = "flake.nix";
            programs = {
              mdformat.enable = true;
              nixpkgs-fmt.enable = true;
              rustfmt = {
                enable = true;
                package = config.nci.toolchains.build;
              };
              shfmt = {
                enable = true;
                indent_size = 0;
              };
            };
          };
        };
      flake = {
        # The usual flake attributes can be defined here, including system-
        # agnostic ones like nixosModule and system-enumerating ones, although
        # those are more easily expressed in perSystem.
      };
    };
}

