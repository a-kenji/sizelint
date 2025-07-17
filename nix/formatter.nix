{ inputs, ... }:
{
  imports = [ inputs.treefmt-nix.flakeModule ];

  perSystem = _: {
    treefmt = {
      projectRootFile = ".git/config";
      programs.mdsh.enable = false;
      programs.nixfmt.enable = true;
      programs.rustfmt.enable = true;
      programs.taplo.enable = true;
      programs.typos.enable = true;
    };
  };
}
