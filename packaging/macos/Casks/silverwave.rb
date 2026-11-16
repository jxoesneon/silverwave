cask "silverwave" do
  version "1.0.0"
  sha256 "1111111111111111111111111111111111111111111111111111111111111111"

  url "https://github.com/jxoesneon/silverwave/releases/download/v#{version}/SilverWave-#{version}-universal2.dmg"
  name "SilverWave"
  desc "Algorithmic live-coding pattern synthesizer, REPL, and VST3/CLAP audio plugin"
  homepage "https://github.com/jxoesneon/silverwave"

  depends_on macos: ">= :big_sur"

  app "SilverWave.app"
  binary "#{appdir}/SilverWave.app/Contents/MacOS/silverwave"
  vst3_plugin "SilverWave.vst3"
  artifact "SilverWave.clap", target: "#{HOMEBREW_PREFIX}/Library/Audio/Plug-Ins/CLAP/SilverWave.clap"

  zap trash: [
    "~/Library/Application Support/silverwave",
    "~/Library/Caches/silverwave",
    "~/Library/Preferences/audio.silverwave.app.plist",
    "~/Library/Saved Application State/audio.silverwave.app.savedState",
  ]
end
