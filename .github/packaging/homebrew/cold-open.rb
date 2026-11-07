# Cold Open's Homebrew cask (docs/specs/release.md §11.3), for our own tap,
# TheYungStump/homebrew-tap: Homebrew's official casks don't take apps that
# fail Gatekeeper, and Cold Open isn't notarized yet. The release workflow
# fills the @…@ values (stores.py); the result goes in the tap's Casks/.
cask "cold-open" do
  version "@VERSION@"
  sha256 "@SHA_MACOS_UNIVERSAL@"

  url "https://github.com/TheYungStump/cold-open/releases/download/v#{version}/ColdOpen-#{version}-macos-universal.dmg"
  name "Cold Open"
  desc "Screenwriting app that reads and writes Fountain and .fdx files"
  homepage "https://theyungstump.github.io/cold-open/"

  livecheck do
    url :url
    strategy :github_releases
  end

  depends_on macos: :monterey

  app "Cold Open.app"
  binary "#{appdir}/Cold Open.app/Contents/MacOS/coldopen"

  zap trash: "~/Library/Application Support/Cold Open"

  caveats <<~EOS
    Cold Open isn't notarized by Apple yet, so macOS refuses it the first time.
    Open System Settings > Privacy & Security and click "Open Anyway",
    then open Cold Open again.
  EOS
end
