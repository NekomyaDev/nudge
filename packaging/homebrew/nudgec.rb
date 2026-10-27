class Nudgec < Formula
  desc "Typed, replayable, budget-aware programming language for AI agents"
  homepage "https://github.com/NekomyaDev/nudge"
  version "1.4.0"
  license "Apache-2.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/NekomyaDev/nudge/releases/download/v1.4.0/nudgec-v1.4.0-macos-aarch64.tar.gz"
    else
      url "https://github.com/NekomyaDev/nudge/releases/download/v1.4.0/nudgec-v1.4.0-macos-x86_64.tar.gz"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/NekomyaDev/nudge/releases/download/v1.4.0/nudgec-v1.4.0-linux-x86_64.tar.gz"
    end
  end

  def install
    bin.install "nudgec"
  end

  test do
    assert_match "nudgec", shell_output("#{bin}/nudgec --help")
    (testpath/"hello.ndg").write <<~EOS
      fn main() -> string {
        "hello"
      }
    EOS
    system "#{bin}/nudgec", "check", testpath/"hello.ndg"
  end
end
