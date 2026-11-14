{
  description = "A Rust implementation of the Concise data definition language (CDDL)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      nixpkgs,
      rust-overlay,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        # The playground and LSP build the library for wasm32; clippy and
        # rust-analyzer read the standard library source.
        toolchainComponents = {
          extensions = [ "rust-src" ];
          targets = [ "wasm32-unknown-unknown" ];
        };
        rustToolchain = pkgs.rust-bin.stable.latest.default.override toolchainComponents;
        # Matches `rust-version` in Cargo.toml.
        msrvToolchain = pkgs.rust-bin.stable."1.88.0".default.override toolchainComponents;
        nativeBuildInputs = with pkgs; [
          rustToolchain
          pkg-config
        ];

        buildInputs = with pkgs; [
          glibc
        ];

        # Cargo resolves subcommands such as `cargo clippy` and `cargo fmt` from
        # $CARGO_HOME/bin before PATH. A shell-specific CARGO_HOME keeps rustup
        # proxies in ~/.cargo/bin from replacing this toolchain's subcommands.
        shellHook = ''
          export CARGO_HOME="''${XDG_CACHE_HOME:-$HOME/.cache}/cddl-devshell/cargo"
        '';
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          inherit nativeBuildInputs buildInputs;
          pname = "cddl";
          version = "0.10.7";

          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
            allowBuiltinFetchGit = true;
          };

          meta = with pkgs.lib; {
            description = "Parser for the Concise data definition language (CDDL)";
            homepage = "https://cddl.anweiss.tech";
            license = licenses.mit;
            maintainers = [ "Andrew Weiss <andrew.weiss@outlook.com>" ];
          };
        };

        devShells.default = pkgs.mkShell {
          inherit shellHook;
          buildInputs = nativeBuildInputs ++ buildInputs;
          # AGENTS.md setup: Node 24, Python 3.11+, wasm-pack, the ruby cddl gem
          # that tests/range_endpoints.rs cross-checks verdicts against, and the
          # cargo-audit and gitleaks scanners that `npm run check:security` runs.
          packages = with pkgs; [
            cargo-audit
            cddl
            gitleaks
            nodejs_24
            python3
            wasm-pack
          ];
          # `npm run test:e2e` runs Playwright against these browsers instead of
          # downloading its own into ~/.cache/ms-playwright.
          env = {
            PLAYWRIGHT_BROWSERS_PATH = "${pkgs.playwright-driver.browsers}";
            PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS = "true";
          };
        };

        devShells.msrv = pkgs.mkShell {
          inherit shellHook;
          buildInputs = [
            msrvToolchain
            pkgs.pkg-config
          ]
          ++ buildInputs;
        };
      }
    );
}
