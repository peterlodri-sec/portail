class Portail < Formula
  desc "Unified proxy/gateway: AI Gateway + MCP Gateway + CDN cache"
  homepage "https://github.com/peterlodri-sec/portail"
  version "2.1.0"
  url "https://github.com/peterlodri-sec/portail/archive/refs/tags/v2.1.0.tar.gz"
  sha256 "64dffd740f4b07063a15a1dfc2f472b66ec371c260b372c2657e9b85ba89769d"
  license "MIT"

  depends_on "pkg-config" => :build
  depends_on "openssl@3" => :build
  depends_on "zlib" => :build
  depends_on "zstd" => :build
  depends_on "rust" => :build

  def install
    # Use Nix build if available for reproducibility, otherwise cargo install
    if ENV["NIX_PATH"] || Dir.exist?("/nix/store")
      # Nix environment - use standard cargo install
      system "cargo", "install", *std_cargo_args
    else
      # Native macOS build with Homebrew dependencies
      env = {
        "PKG_CONFIG_PATH" => "#{Formula["pkg-config"].prefix}/lib/pkgconfig",
        "OPENSSL_LIB_DIR" => "#{Formula["openssl@3"].opt_lib}",
        "OPENSSL_INCLUDE_DIR" => "#{Formula["openssl@3"].opt_include}",
        "ZLIB_HOME" => "#{Formula["zlib"].prefix}",
        "ZSTD_HOME" => "#{Formula["zstd"].prefix}"
      }
      
      env.each { |k, v| ENV[k] = v }
      
      system "cargo", "install", *std_cargo_args
    end
  end

  def post_install
    # Create necessary directories
    (var/"log/portail").mkpath
    (var/"run/portail").mkpath if !Dir.exist?("/run/portail")
  end

  test do
    assert_match /portail/, shell_output("#{bin}/portail --version")
    assert_match /proxy.*gateway.*ai.*mcp.*cdn/i, shell_output("#{bin}/portail --help")
  end
end
