{ lib, stdenv, rustPlatform, callPackage, fetchFromGitHub
, installShellFiles, pkg-config, openssl, zlib, zstd, upx
}:

let
  pname = "portail";
  version = "2.0.0";
in
rustPlatform.buildRustPackage {
  inherit pname version;

  src = ../.;
  cargoLock.lockFile = ../Cargo.lock;

  nativeBuildInputs = [ installShellFiles pkg-config upx ];
  buildInputs = [ openssl zlib ]
    ++ lib.optionals stdenv.isLinux [ zstd ];

  # 🚀 Production optimization flags
  env = {
    CARGO_PROFILE_RELEASE_LTO = "fat";
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS = "1";
    CARGO_PROFILE_RELEASE_OPT_LEVEL = "3";
    CARGO_PROFILE_RELEASE_STRIP = "symbols";
    CARGO_PROFILE_RELEASE_DEBUG = "false";
    CARGO_PROFILE_BENCH_LTO = "fat";
    CARGO_PROFILE_BENCH_CODEGEN_UNITS = "1";
  };

  # Build only the main binary (not test benches or mon)
  cargoBuildFlags = [ "--bin" "portail" ];

  checkFlags = [ "--lib" "--bins" "--tests" ];

  # 🔒 UPX-compress the release binary (best ratio, LZMA for smallest output)
  postInstall = ''
    upx --best --lzma $out/bin/portail
  '';

  # Python MCP plugin — installed as a sibling package
  passthru.mcpPlugin = callPackage ./mcp-plugin.nix { };

  meta = {
    description = "Unified proxy/gateway: AI Gateway + MCP Gateway + CDN cache";
    homepage = "https://github.com/peterlodri-sec/portail";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux ++ lib.platforms.darwin;
    maintainers = [ ];
    mainProgram = "portail";
  };
}
