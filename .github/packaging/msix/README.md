# Microsoft Store (MSIX)

Cold Open's MSIX package (docs/specs/release.md §11.4).

| File | What it does |
| --- | --- |
| `AppxManifest.xml` | The package manifest: the identity, `runFullTrust`, the three file types, and the `coldopen` alias for the CLI |
| `make_assets.py` | The Store's logos, drawn by the icon's own code |
| `build_msix.ps1` | Packs one architecture's MSIX from a release's files |
| `store_bundle.ps1` | The release's Store upload: x64 and ARM64, joined in an `.msixbundle` |
| `ci_msix.ps1`, `ci_winget.ps1` | CI: packs, signs with a throwaway certificate, installs and starts it; validates the winget manifests |

**The identity** comes from the repository variables `MSIX_IDENTITY_NAME`, `MSIX_PUBLISHER` and `MSIX_PUBLISHER_DISPLAY`, set on 2026-11-06 from Partner Center's reservation (display name `TheYungStump`). Each release run then makes the **store-msix** artifact.

**The owner's steps, the listing text and the certification notes** are in `docs/marketing/microsoft-store.md`.
