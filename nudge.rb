class Nudge < Formula
  desc "Typed, replayable, budget-aware programming language for LLM agents"
  homepage "https://github.com/NekomyaDev/nudge"
  version "1.2.0"
  license "Proprietary"

  on_macos do
    on_arm do
      url "https://github.com/NekomyaDev/nudge/releases/download/v1.2.0/nudgec-v1.2.0-macos-aarch64.tar.gz"
      sha256 "43ba9e49616a8bb922bd9d28b614d26553d75603e9f5f52ab6e0bb1ffb874bed"
    end

    on_intel do
      url "https://github.com/NekomyaDev/nudge/releases/download/v1.2.0/nudgec-v1.2.0-macos-x86_64.tar.gz"
      sha256 "5cd7f0778f6cef364c2decd7beec038b64b6c6cf7164a49a0a7fb68b5eaca2d4"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/NekomyaDev/nudge/releases/download/v1.2.0/nudgec-v1.2.0-linux-x86_64.tar.gz"
      sha256 "0b05a3ac75c853cb80f5576f15059291ea05e4b83be12f6661d138b3fb476b65"
    end
  end

  def install
    bin.install "nudgec"
  end

  test do
    system "#{bin}/nudgec", "--version"
  end
end
