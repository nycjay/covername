# Homebrew formula for Covername CLI
#
# To install:
#   brew tap nycjay/tap
#   brew install covername
#
# This formula builds from source. Requires Rust toolchain.

class Covername < Formula
  desc "Local-first document anonymization tool — detect and replace PII with cover identities"
  homepage "https://github.com/nycjay/covername"
  url "https://github.com/nycjay/covername/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "" # Updated on release
  license "MIT"
  head "https://github.com/nycjay/covername.git", branch: "main"

  depends_on "rust" => :build
  depends_on "tesseract" => :recommended

  def install
    system "cargo", "install", *std_cargo_args(path: "crates/covername-cli")
  end

  test do
    # Verify the binary runs
    assert_match "covername", shell_output("#{bin}/covername --version")

    # Test scanning a simple text file
    (testpath/"test.txt").write("Call John Smith at 555-123-4567")
    output = shell_output("#{bin}/covername scan #{testpath}/test.txt")
    assert_match "PHONE", output
  end
end
