# SPDX-FileCopyrightText: 2026 Harish Rajagopal <harish.rajagopals@gmail.com>
#
# SPDX-License-Identifier: AGPL-3.0-or-later
{
  description = "Simple viewer webpage for Dilbert by Scott Adams";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
  };

  outputs =
    { nixpkgs, ... }:
    let
      forAllSystems =
        function:
        nixpkgs.lib.genAttrs [ "x86_64-linux" ] (system: function (import nixpkgs { inherit system; }));
    in
    {
      packages = forAllSystems (pkgs: {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "dilbert-viewer";
          version = "0.4.0";

          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;

          # Ship the static assets next to the binary.
          postInstall = ''
            install -d $out/share/dilbert-viewer/static
            cp -r ./static/. $out/share/dilbert-viewer/static
          '';

          meta = with pkgs.lib; {
            description = "Simple viewer webpage for Dilbert by Scott Adams";
            homepage = "https://dilbert-viewer.rharish.dev";
            repository = "https://github.com/rharish101/dilbert-viewer";
            license = licenses.agpl3Plus;
            platforms = platforms.all;
            mainProgram = "dilbert-viewer";
          };
        };
      });
    };
}
