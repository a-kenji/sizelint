_: {
  perSystem =
    { self', ... }:
    {
      packages = rec {
        default = sizelint;
        inherit (self'.checks) sizelint;
      };
    };
}
