"""CI's Flathub manifest (ci.yml, store-flathub): the release manifest,
building this checkout instead of the tag, with a stand-in license list."""

p = ".github/packaging/flathub/io.github.theyungstump.coldopen.yml"
s = open(p, encoding="utf-8").read()
# The metainfo line's split names stay as they are; only the values change.
s = s.replace("|@VERSION@|", "|0.0.0-ci|").replace("|@DATE@|", "|2026-01-01|")
s = s.replace("@VERSION@", "0.0.0-ci").replace("@COMMIT@", "ci")
i = s.index("    sources:\n")
s = s[:i] + (
    "    sources:\n"
    "      - type: dir\n"
    "        path: ../../..\n"
    "      - type: file\n"
    "        path: third-party.txt\n"
    "      - cargo-sources.json\n"
)
open(".github/packaging/flathub/ci.yml", "w", encoding="utf-8", newline="\n").write(s)
print("wrote .github/packaging/flathub/ci.yml")
