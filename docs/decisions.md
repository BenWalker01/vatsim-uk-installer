# Decisions and Open Questions

## Decisions made (scaffold stage)

| # | Decision | Rationale |
|---|----------|-----------|
| 1 | Rust + egui/eframe for the GUI | Single native binary, no runtime to install; fits the "one stop shop" goal. |
| 2 | Wizard flow: Welcome → EuroScope → VC++ Redist → Controller Pack → Updates → Finish | Matches the requested first-run order. |
| 3 | Pack versions are monotonically increasing integers | Simple to compare and chain; revisit if semver is wanted. |
| 4 | Updates are changes-only patches (`from` → `version`) applied sequentially | Matches how the pack ships. `updater::plan` walks the chain. |
| 5 | A missing link in the patch chain, or no installed version, triggers baseline reinstall + patches | Safe fallback for corrupt or very old installs. |
| 6 | Installer state stored in `%APPDATA%\vatsim-uk-installer\state.json` | Tracks installed pack version and paths. |
| 7 | A remote JSON manifest drives EuroScope, VC++ and pack downloads | Lets releases change without rebuilding the installer. |
| 8 | VC++ check targets the x86 2015-2022 runtime (registry) | EuroScope is 32-bit. |
| 9 | EuroScope must be an exact version (`required_version`), not a minimum | Newer releases are less stable; both older and newer versions are flagged. |

## Open questions

1. **Manifest hosting:** where does the manifest live (GitHub releases, VATSIM UK web server, CDN)? `MANIFEST_URL` is a placeholder.
2. **Patch format:** zip of changed files plus a deletions list? Should patches carry file-level hashes for verification?
3. **Version scheme:** are pack versions integers, dates (e.g. AIRAC-style) or semver?
4. **Local edits:** how do we treat user-modified files in the pack (overwrite, back up, or skip)? This matters for the later configuration work.
5. **EuroScope version policy:** which exact version is required? Should a mismatched install block progress or only warn, and do we offer to downgrade (uninstall/replace) a newer one?
6. **EuroScope install location:** do we support non-default install paths (user picker), and how do we find the sector files folder?
7. **Elevation:** the VC++ redist needs admin. Should the installer always run elevated, or elevate only for that step?
8. **Installer self-update:** should the installer check for its own new versions on startup (as VSEDI does)?
9. **Controller pack config:** fold the existing standalone configuration app in as a wizard step, or keep it separate? (planned for later)
10. **Platform support:** Windows only, or Linux (Wine) later?
11. **Distribution:** code signing, packaging (plain exe vs MSI), and release pipeline.
12. **Offline/failure behaviour:** what if the manifest is unreachable (currently an offline banner)? Cache the last manifest?
13. **Integrity:** SHA-256 per download is planned; do we also want signed manifests?

## Next implementation steps

- HTTP downloader with progress reporting and SHA-256 verification
- Controller pack baseline install and patch application (`pack.rs`)
- EuroScope exe version read and install flow
- VC++ redist silent install (`/install /quiet /norestart`)
- Manifest fetching and caching
