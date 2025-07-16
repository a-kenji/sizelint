{
  self,
  lib,
  pkgs,
}:
let
  cargoTOML = builtins.fromTOML (builtins.readFile (self + "/Cargo.toml"));
  inherit (cargoTOML.package) version name;
  pname = name;
  gitDate = "${builtins.substring 0 4 self.lastModifiedDate}-${
    builtins.substring 4 2 self.lastModifiedDate
  }-${builtins.substring 6 2 self.lastModifiedDate}";
  gitRev = self.shortRev or self.dirtyShortRev;
  meta = import ./meta.nix { inherit lib; };
  craneLib = self.inputs.crane.mkLib pkgs;
  commonArgs = {
    nativeBuildInputs = with pkgs; [
      scdoc
      installShellFiles
    ];
    inherit version name pname;
    src = lib.cleanSourceWith { src = craneLib.path ../.; };
  };
  cargoArtifacts = craneLib.buildDepsOnly commonArgs;
  cargoClippy = craneLib.cargoClippy (commonArgs // { inherit cargoArtifacts; });
  cargoDeny = craneLib.cargoDeny (commonArgs // { inherit cargoArtifacts; });
  cargoTarpaulin = craneLib.cargoTarpaulin (commonArgs // { inherit cargoArtifacts; });
  cargoDoc = craneLib.cargoDoc (commonArgs // { inherit cargoArtifacts; });
  cargoTest = craneLib.cargoNextest (commonArgs // { inherit cargoArtifacts; });
in
{
  sizelint = craneLib.buildPackage (
    commonArgs
    // {
      cargoExtraArgs = "-p ${name}";
      env = {
        GIT_DATE = gitDate;
        GIT_REV = gitRev;
      };
      doCheck = false;
      version = version + "-unstable-" + gitDate;
      postInstall = ''
        # Generate and install shell completions
        installShellCompletion --cmd sizelint \
          --bash <($out/bin/sizelint completions bash) \
          --fish <($out/bin/sizelint completions fish) \
          --zsh <($out/bin/sizelint completions zsh)

        # Build and install manpage
        scdoc < ${self}/docs/sizelint.1.scd > sizelint.1
        installManPage sizelint.1
      '';
      inherit
        name
        pname
        cargoArtifacts
        meta
        ;
    }
  );
  inherit
    cargoClippy
    cargoArtifacts
    cargoDeny
    cargoTarpaulin
    cargoDoc
    cargoTest
    ;
}
