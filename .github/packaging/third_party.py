"""Writes THIRD-PARTY-LICENSES.txt from cargo-about's JSON
(docs/specs/release.md §10):

    cargo about generate --workspace --locked --format json --target <triple> \\
        | python3 .github/packaging/third_party.py > THIRD-PARTY-LICENSES.txt

Our own crates (path dependencies, with no `source`) are left out: their
licenses are in the files beside this one. Each license text is printed
once, with the crates it covers; the output is sorted, so it depends only
on Cargo.lock and the target.
"""

import json
import sys


def main():
    data = json.load(sys.stdin)
    by_text = {}
    for lic in data.get("licenses", []):
        crates = sorted(
            {
                (u["crate"]["name"], u["crate"]["version"])
                for u in lic.get("used_by", [])
                if u["crate"].get("source")
            }
        )
        if not crates:
            continue
        text = lic["text"].replace("\r\n", "\n").strip("\n")
        key = (lic["id"], text)
        entry = by_text.setdefault(key, {"name": lic["name"], "crates": set()})
        entry["crates"].update(crates)
    if not by_text:
        raise SystemExit("no third-party licenses found: is the JSON from cargo about?")

    out = sys.stdout
    out.write("Third-party licenses\n====================\n\n")
    out.write(
        "Cold Open is built with the open-source crates below. Each one's\n"
        "license follows the list of crates it covers.\n\n"
    )
    total = len({c for e in by_text.values() for c in e["crates"]})
    texts = "license text" if len(by_text) == 1 else "license texts"
    out.write(f"{total} crates under {len(by_text)} {texts}.\n")
    ordered = sorted(by_text.items(), key=lambda kv: (kv[0][0], sorted(kv[1]["crates"])))
    for (lic_id, text), entry in ordered:
        out.write("\n\n" + "-" * 72 + "\n")
        out.write(f"{entry['name']} ({lic_id})\n\nUsed by:\n")
        for name, version in sorted(entry["crates"]):
            out.write(f"  {name} {version}\n")
        out.write("\n" + text + "\n")


if __name__ == "__main__":
    main()
