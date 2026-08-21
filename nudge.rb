class Nudge < Formula
  desc "Typed, replayable, budget-aware programming language for LLM agents"
  homepage "https://github.com/NekomyaDev/nudge"
  version "1.2.0"
  license "Proprietary"

  on_macos do
    on_arm do
      url "https://github.com/NekomyaDev/nudge/releases/download/v1.2.0/nudgec-v1.2.0-macos-aarch64.tar.gz"
      sha256 "PLACEHOLDER_SHA256"
    end

    on_intel do
      url "https://github.com/NekomyaDev/nudge/releases/download/v1.2.0/nudgec-v1.2.0-macos-x86_64.tar.gz"
      sha256 "PLACEHOLDER_SHA256"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/NekomyaDev/nudge/releases/download/v1.2.0/nudgec-v1.2.0-linux-x86_64.tar.gz"
      sha256 "PLACEHOLDER_SHA256"
    end
  end

  def install
    bin.install "nudgec"
  end

  test do
    system "#{bin}/nudgec", "--version"
  end
end
