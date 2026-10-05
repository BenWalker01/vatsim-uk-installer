# Decisions and Open Questions

## Decisions made (scaffold stage)

| # | Decision | Rationale |
|---|----------|-----------|
| 1 | Rust + egui/eframe for the GUI | Single native binary, no runtime to install; fits the "one stop shop" goal. |
| 2 | Wizard flow: Welcome → EuroScope → VC++ Redist → Controller Pack → Updates → Finish | Matches the requested first-run order. |
| 3 | Pack versions are the GitHub release tags: `YYYY_MM` plus optional hotfix letter (`2026_09a`), ordered chronologically (`PackVersion`) | Matches https://github.com/VATSIM-UK/uk-controller-pack/releases. |
| 4 | Each release ships `uk_controller_pack_<tag>.zip` (full) and `changes_only_<tag>.zip` (diff vs previous release); updating applies every release after the installed one, oldest first | Verified against the 2026_10 release. `updater::plan` does this. |
| 5 | No installed version, or an installed tag not in the release list, triggers a full install of the latest release | Safe fallback; no patches needed since the full zip is current. |
| 6 | Installer state stored in `%APPDATA%\vatsim-uk-installer\state.json` | Tracks installed pack version and paths. |
| 7 | Pack releases come from the GitHub releases API (assets expose a `sha256:` digest used for verification); EuroScope/VC++ info needs its own source | Avoids maintaining a separate pack manifest. |
| 8 | VC++ check targets the x86 2015-2022 runtime (registry) | EuroScope is 32-bit. |

| 9 | EuroScope must be an exact version (`required_version`), not a minimum | Newer releases are less stable; both older and newer versions are flagged. |
| 10 | Every tag has both zips (answered) | No need to handle missing assets beyond erroring clearly. |
| 11 | Limit GitHub API usage: one `releases` list call per run (cached on disk, use `ETag`/`If-None-Match`), then download zips via `browser_download_url` | Unauthenticated API is 60 req/h; asset downloads from `browser_download_url` are not API calls, and zips are fetched whole, never per file. |
| 12 | Controller pack lives in `%APPDATA%\EuroScope\UK` (`pack::default_dir`) | User requirement. Both zips contain a top-level `UK/` folder (plus `README.pdf`); the installer strips `UK/` and extracts into the pack dir, skipping README.pdf. |
| 13 | Downloads are verified against the GitHub asset `sha256:` digest; installed version is saved after each applied release | An interrupted update resumes from the last good release. |

## Open questions

1. **EuroScope/VC++ source:** pack data comes from GitHub; where do the required EuroScope version and download URLs live (hard-coded, small JSON in the repo, web server)?
2. **Deleted files:** `changes_only_*.zip` does not list deletions (can be added to the release workflow if needed). Until then removed files linger after an update; decide whether to add a deletions list (e.g. `deleted.txt` in the zip).
3. **Local edits:** how do we treat user-modified files in the pack (overwrite, back up, or skip)? This matters for the later configuration work.
4. **EuroScope version policy:** required exact version is `3.2.3.2` (hard-coded in `Manifest::fetch`). Should a mismatched install block progress or only warn, and do we offer to downgrade (uninstall/replace) a newer one? Where does the download URL come from?
5. **EuroScope install location:** do we support non-default install paths (user picker), and how do we find the sector files folder?
6. **Elevation:** the VC++ redist needs admin. Should the installer always run elevated, or elevate only for that step?
7. **Installer self-update:** should the installer check for its own new versions on startup (as VSEDI does)?
8. **Controller pack config:** fold the existing standalone configuration app in as a wizard step, or keep it separate? (planned for later)
9. **Platform support:** Windows only, or Linux (Wine) later?
10. **Distribution:** code signing, packaging (plain exe vs MSI), and release pipeline.
11. **Offline/failure behaviour:** what if the manifest is unreachable (currently an offline banner)? Cache the last manifest?
12. **Integrity:** SHA-256 per download is planned; do we also want signed manifests?

## Next implementation steps

- HTTP downloader with progress reporting and SHA-256 verification
- Controller pack baseline install and patch application (`pack.rs`)
- EuroScope exe version read and install flow
- VC++ redist silent install (`/install /quiet /norestart`)
- Manifest fetching and caching
