class Uc < Formula
  desc "Memoryport — persistent LLM memory on Arweave"
  homepage "https://github.com/t8/memoryport"
  version "0.1.0"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "https://github.com/t8/memoryport/releases/download/v#{version}/memoryport-macos-arm64.tar.gz"
      # sha256 "PLACEHOLDER"
    end
    on_intel do
      url "https://github.com/t8/memoryport/releases/download/v#{version}/memoryport-macos-x64.tar.gz"
      # sha256 "PLACEHOLDER"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/t8/memoryport/releases/download/v#{version}/memoryport-linux-arm64.tar.gz"
      # sha256 "PLACEHOLDER"
    end
    on_intel do
      url "https://github.com/t8/memoryport/releases/download/v#{version}/memoryport-linux-x64.tar.gz"
      # sha256 "PLACEHOLDER"
    end
  end

  def install
    bin.install "uc"
    bin.install "uc-mcp"
    bin.install "uc-proxy"
    bin.install "uc-server"
  end

  test do
    assert_match "Unlimited Context", shell_output("#{bin}/uc --help")
  end
end
