class Portail < Formula
  desc "Unified proxy/gateway: AI Gateway + MCP Gateway + CDN cache"
  homepage "https://github.com/peterlodri-sec/portail"
  url "https://github.com/peterlodri-sec/portail/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "PLACEHOLDER_SHA256"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "portail", shell_output("#{bin}/portail --version")
  end
end
