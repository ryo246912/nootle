# Homebrew cask for Nootle. This repository doubles as a tap:
#
#   brew tap ryo246912/nootle https://github.com/ryo246912/nootle
#   brew install --cask nootle
#
# The release workflow bumps version and sha256 on every release. This is the
# ryo246912 fork's cask, so it must keep pointing at the fork's releases.
cask "nootle" do
  version "0.37.1"
  sha256 "de71bbe8afa85634f73bb84ca480e65addabb8ad99e7c6bff74dec133b761cea"

  url "https://github.com/ryo246912/nootle/releases/download/v#{version}/Nootle_#{version}_aarch64.dmg"
  name "Nootle"
  desc "Local-first AI meeting recorder with private transcription and summaries"
  homepage "https://github.com/ryo246912/nootle"

  livecheck do
    url :url
    strategy :github_latest
  end

  auto_updates true
  depends_on arch: :arm64
  depends_on macos: ">= :sonoma"

  app "Nootle.app"

  # Releases are not notarized unless the fork has Apple credentials, and
  # Homebrew quarantines downloads, so Gatekeeper would refuse to open the app.
  postflight do
    system_command "/usr/bin/xattr",
                   args: ["-dr", "com.apple.quarantine", "#{appdir}/Nootle.app"],
                   must_succeed: false
  end

  zap trash: [
    "~/Library/Application Support/Nootle",
    "~/Library/Caches/com.nootle.desktop",
    "~/Library/Logs/Nootle",
    "~/Library/Preferences/com.nootle.desktop.plist",
    "~/Library/Saved Application State/com.nootle.desktop.savedState",
    "~/Library/WebKit/com.nootle.desktop",
  ]
end
