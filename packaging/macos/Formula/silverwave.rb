class Silverwave < Formula
  desc "Algorithmic live-coding pattern engine and synthesizer CLI"
  homepage "https://github.com/jxoesneon/silverwave"
  url "https://github.com/jxoesneon/silverwave/archive/refs/tags/v0.8.0.tar.gz"
  sha256 "0000000000000000000000000000000000000000000000000000000000000000"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/jxoesneon/silverwave.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args(path: "crates/silverwave-cli")
    generate_completions_from_executable(bin/"silverwave", "completions")
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/silverwave --version")
    system bin/"silverwave", "eval", "--expr", "s(\"bd sd\")", "--cycles", "0..1", "--format", "json"
  end
end
