{ self, ... }:
{
  perSystem =
    { pkgs, ... }:
    {
      checks = {
        inherit ((pkgs.callPackage ./crane.nix { inherit self; }))
          sizelint
          cargoArtifacts
          cargoClippy
          cargoDoc
          cargoTest
          cargoTarpaulin
          ;
      };
    };
}
