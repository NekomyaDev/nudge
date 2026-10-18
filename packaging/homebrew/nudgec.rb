class Nudgec < Formula
  desc "Typed, replayable, budget-aware programming language for AI agents"
  homepage "https://github.com/NekomyaDev/nudge"
  version "1.2.1"
  license "Apache-2.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/NekomyaDev/nudge/releases/download/v1.2.0/nudgec-v1.2.0-macos-aarch64.tar.gz"
      sha256 "43ba9e49616a8bb922bd9d28b614d26553d75603e9f5f52ab6e0bb1ffb874bed"
    else
      url "https://github.com/NekomyaDev/nudge/releases/download/v1.2.0/nudgec-v1.2.0-macos-x86_64.tar.gz"
      sha256 "5cd7f0778f6cef364c2decd7beec038b64b6c6cf7164a49a0a7fb68b5eaca2d4"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/NekomyaDev/nudge/releases/download/v1.2.1/nudgec-v1.2.1-linux-x86_64.tar.gz"
      sha256 "8bb5b0c057c8018abe8bed811bb6a45994a4df4f0fe62f85a832218439df9d9c"
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
