# Nix / NixOS

From the repository root:

```bash
nix build
nix run
```

Install into the current user profile with `nix profile install .`. NixOS users
can add the flake as an input and include `pixelkit.packages.${system}.default`
in `environment.systemPackages`. Enable the packaged user service separately if
global shortcuts should start with the graphical session.
