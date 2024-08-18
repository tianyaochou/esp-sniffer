{
  inputs.flake-parts.url = "github:hercules-ci/flake-parts";
  inputs.devenv.url = "github:cachix/devenv";
  inputs.fenix.url = "fenix";

  outputs = inputs@{ flake-parts, nixpkgs, ... }:
    flake-parts.lib.mkFlake { inherit inputs; }
    {
      imports = [ inputs.devenv.flakeModule ];
      systems = [ "x86_64-linux" "x86_64-darwin" ];
      perSystem = { pkgs, lib, ... }:{
        devenv.shells.default = {
          packages = with pkgs; [ probe-rs espflash ];
          env = {
          };
          languages.rust = {
            enable = true;
            channel = "nightly";
            targets = [ "riscv32imac-unknown-none-elf" ];
            components = [ "rustc" "cargo" "clippy" "rustfmt" "rust-analyzer" "rust-src" ];
          };
        };
      };
    };
}
