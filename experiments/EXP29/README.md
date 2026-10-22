# EXP29 - a per-user Windows installer next to the zip

**Status:** `success` (user, 2026-10-22).

## Summary
Ship `AirQlality-v<version>-windows-x64-setup.exe` (Inno Setup, per-user, no
UAC) alongside the portable zip: Start Menu + optional desktop shortcut, an
Add/Remove Programs entry with the app icon, clean uninstall, in-place upgrade.

## Hypothesis
The app is one self-contained exe with no runtime DLLs, so a single Inno Setup
script can turn it into an installer that - built on this box and on a stock
`windows-2025` runner - provably:

- installs silently (`/VERYSILENT`) into `%LocalAppData%\Programs\AirQlality`
  without elevation, landing the exe plus a Start Menu shortcut;
- leaves an Add/Remove Programs entry that names the version and shows the
  EXP14 icon, and an installed exe that still passes the existing widget E2E;
- upgrades in place over an older version, one entry, no duplicate shortcuts;
- uninstalls silently to nothing: no exe, no shortcut, no ARP entry and **no
  dangling `HKCU\...\Run` value** (EXP12), while keeping
  `%APPDATA%\AirQlality` settings.

**Falsified if:** installing needs elevation, the installed exe fails to render
the widget, an uninstall leaves the exe/shortcut/ARP entry behind, an autostart
value survives it, or the setup cannot be produced in CI without a human.

## Methodology & blast radius
- **Method**
  1. `app/examples/emit_icon.rs`: write the EXP14 icon to a real file
     (`icon_design::encode_ico`, same `#[path]` include trick as `build.rs`),
     because the `.ico` the build script makes lives in `OUT_DIR` - no new crate.
  2. `installer/airqlality.iss`: `AppId` GUID, `PrivilegesRequired=lowest`,
     `DefaultDirName={localappdata}\Programs\AirQlality`, `AppVersion` injected
     from the tag (`/DAppVersion=`), `SetupIconFile` = the generated icon,
     `UninstallDisplayIcon` = the installed exe, `CloseApplications=yes`
     (upgrade over a running tray app), Start Menu shortcut + optional desktop
     icon task, optional "launch now" on the finish page, `[Code]`
     `CurUninstallStepChanged` deleting the `HKCU\...\Run` value, and
     `UninstallDelete` leaving `%APPDATA%\AirQlality` alone.
  2b. `installer/stop-app.ps1` + `PrepareToInstall`: the widget hides to the tray
     on `WM_CLOSE` (EXP11), so Restart Manager cannot end it and a silent upgrade
     over a running copy died with **exit 5**. Setup now extracts this script to
     `{tmp}` and stops only the copy running from `{app}\airqlality.exe` before
     replacing it; a portable copy elsewhere keeps running.
  3. `.github/workflows/release.yml`: install Inno Setup on the runner (not
     preinstalled on `windows-2025`), build the setup from the `dist` exe,
     upload it next to the zip, and hash **both** in `SHA256SUMS.txt`.
  4. Tests: a PowerShell proof script that installs silently into a scratch
     dir, asserts exe + shortcut + ARP entry + version, launches the installed
     exe on the mock source through the existing `app/tests/capture.ps1`,
     uninstalls silently, then asserts exe/shortcut/ARP/Run value gone and
     settings kept. CI runs the install/uninstall half headless.
  5. README: "Download" offers the installer first, the zip as the portable
     option, and states the unsigned-SmartScreen caveat for both.
- **Evidence:** `EXP29-setup.png` (the wizard a user sees: MIT licence page, the
  EXP14 icon in the title bar and on the setup exe - verified by sampling the
  extracted icon: `#f85149` / `#d29922` / `#3fb950` / `#8b949e` in the four
  quadrants), `EXP29-installed-widget.png` (the widget launched from the
  installed `%LOCALAPPDATA%\Programs\AirQlality` path, mock source, KEEP CLOSED),
  `EXP29-proof.log` (28 assertions, `RESULT PASS`, run from a **non-elevated**
  shell: `elevated=False`) - including an upgrade over a running widget (setup
  exit 0, the running copy closed, a portable copy left alone).
  `EXP29-released-widget.png`: the same proof again on the **published**
  `v0.3.0` setup downloaded from the release (SHA-256 re-checked against
  `SHA256SUMS.txt`), which also proves the `dist` exe starts and renders off the
  build machine. Sizes for the same 32.6 MiB exe: setup **9.9 MiB**
  (`10,367,078` bytes), zip **12.5 MiB** - lzma2/max beats deflate, so the
  installer is the smaller download. Suites: core 64/64, app 82/82, windowed E2E
  33/33 in 276 s - no product code changed, so no re-baseline.
- **May touch:** `installer/airqlality.iss` (new), `app/examples/emit_icon.rs`
  (new), `experiments/EXP29/**`, `.github/workflows/release.yml`,
  `.gitignore`, `README.md`, `experiments-overview.md`, branch `EXP29`.
- **Off-limits:** `core/**`, `app/src/**` behaviour (no tray/startup/notify
  changes), `.github/workflows/ci.yml` gates, the zip asset, `UI.md`,
  `AGENTS.md`, `experiments/EXP01..28/**`, declaring a final status.

## Open questions (known unknowns)
- [x] Installer tech? - **Inno Setup 6** (one script, per-user, silent-friendly,
  no MSI/enterprise need). *(Assumption adopted.)*
- [x] Install scope? - **per-user** `%LocalAppData%\Programs\AirQlality`,
  `PrivilegesRequired=lowest`, no UAC. *(Assumption adopted.)*
- [x] Keep the zip? - **yes, publish both**; the setup is the default download.
  *(Assumption adopted.)*
- [x] Code signing? - **none**; SmartScreen warns, documented, `SignTool`
  hooks left in place for a future certificate. *(Assumption adopted.)*
- [x] Uninstall data policy? - **always clear the `HKCU\Run` value**, **keep**
  `%APPDATA%\AirQlality` (Inno cannot ask at uninstall time cleanly).
  *(Assumption adopted.)*
- [x] Autostart from the installer? - **no**; EXP12's tray toggle stays the only
  writer, the finish page only offers "launch now". *(Assumption adopted.)*
- [x] Installer icon source? - **`examples/emit_icon.rs`**, reusing
  `icon_design::encode_ico`. *(Assumption adopted.)*
- [x] Local verification? - **install Inno Setup locally** (winget, ~7 MB) so
  the screenshot evidence is a real install, not a CI artifact download.
  *(Assumption adopted; user approved the plan.)*
- [x] Version source? - **the git tag**, as `release.yml` already enforces; no
  auto-updater in this experiment. *(Assumption adopted.)*

## Conclusion (findings)
**Success (user, 2026-10-22).** One Inno Setup script turns the single exe into
an installer that behaves like the app: per-user, no admin, no runtime. It is
published as `v0.3.0` and every claim below is asserted by
`installer/proof.ps1` (28 checks), which the `Release` workflow runs against the
setup it just built, *before* uploading it.

What landed:
- `installer/airqlality.iss`: Inno Setup 6, `PrivilegesRequired=lowest`,
  `%LocalAppData%\Programs\AirQlality`, stable `AppId` (one Add/Remove Programs
  entry across versions), version from the tag, the EXP14 icon on the setup and
  the ARP entry, Start Menu shortcut, opt-in desktop shortcut, opt-in "launch
  now", `MinVersion` 10.0.18362 (the README's promise).
- `installer/stop-app.ps1` + `PrepareToInstall`: an upgrade replaces the exe
  while the widget is in the tray. Restart Manager cannot do this (the widget
  hides on `WM_CLOSE`, EXP11, so setup died with **exit 5**); setup now stops only
  the copy running from `{app}\airqlality.exe` and leaves a portable copy alone.
- `installer/proof.ps1`: silent install/upgrade/uninstall with 28 assertions -
  exe, shortcut target, ARP name/version/icon, one entry after an upgrade,
  upgrade over a running widget, portable copy untouched, uninstall leaving no
  exe/shortcut/entry/autostart value, settings kept.
- `app/examples/emit_icon.rs`: the icon only existed in `OUT_DIR`, and Inno wants
  a file - the example reuses `icon_design::encode_ico`, so setup, exe and
  window icon cannot drift.
- `release.yml`: installs Inno on the runner (absent from `windows-2025`), builds
  the setup from the `dist` exe, gates the upload on the proof, and hashes both
  assets; README offers the installer first and the zip as the portable option.
- `v0.3.0` published: setup **9.9 MiB**, zip **12.5 MiB** for the same 32.6 MiB
  exe - the installer is the smaller download, because lzma2/max beats deflate.

Limits / not proven:
- Unsigned: SmartScreen warns once (`More info -> Run anyway`), documented. No
  `SignTool` name is configured, so adding a certificate is a one-line change
  later; no MSI, no per-machine install, no auto-update.
- The proof is a silent-install proof. The wizard itself is only screenshotted at
  the licence page (`EXP29-setup.png`); nobody clicked Next/Install by hand.
- Upgrading stops the widget and does not restart it (silent installs stay
  silent); uninstall always keeps `%APPDATA%\AirQlality` and gives no prompt.
- One unproven one-off: the very first proof run found a desktop shortcut even
  though the task is `unchecked`; every later run (many installs and upgrades)
  never created one. Suspected stale artifact, never reproduced.
- The published setup was also installed and launched here, from the downloaded
  `dist` exe (`EXP29-released-widget.png`) - the same proof, 28/28, SHA-256
  re-checked against `SHA256SUMS.txt`.

## Release
`v0.3.0` published on 2026-10-22 after the `Release` workflow's own 28/28 proof
(`github.com/Magid-William/AirQlality/releases/tag/v0.3.0`): setup, zip and
`SHA256SUMS.txt`. Two earlier runs of the same tag failed on **one** assertion -
the shortcut target compared as a path string against the runner's 8.3 `%TEMP%` -
so the tag was deleted and re-pushed twice; the installer build and proof were
green apart from that assertion both times.

## Learnings
- Inno Setup is not on the `windows-2025` image (it was on 2022):
  `choco install innosetup --no-progress -y`, ISCC at
  `C:\Program Files (x86)\Inno Setup 6\ISCC.exe`; `winget install
  JRSoftware.InnoSetup` on a dev box lands in `%LocalAppData%\Programs\Inno Setup
  6\`. Search both paths.
- `ISCC /DAppVersion=<ver> /DSourceExe=<exe> /DSourceIcon=<ico> installer\airqlality.iss`
  builds the setup; `#ifndef` defaults keep a bare `cargo build --release` tree
  working. `UninstallDisplayName` is what ARP shows (otherwise `AppVerName`), and
  the ARP key is `HKCU\...\Uninstall\{AppId}_is1` - find it by `DisplayName`.
- `PrivilegesRequired=lowest` + `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART
  /DIR=...` proves a no-UAC install from a non-elevated shell, and silent
  installs skip `unchecked` tasks (no desktop icon).
- The Inno uninstaller re-execs itself from `%TEMP%` and returns at once:
  `Start-Process -Wait` proves nothing, poll for the exe to disappear.
- A tray/hide-on-close app cannot be upgraded with `CloseApplications=yes`: RM's
  close hides the window instead of ending it (setup exit 5). Ship `[Files]
  Flags: dontcopy` + `ExtractTemporaryFile` + `Exec(powershell -File stop.ps1)`
  in `PrepareToInstall`, matching the process `Path` so other copies survive.
- Never compare paths with a hosted runner: `%TEMP%` is 8.3 (`RUNNER~1`) while
  shortcuts store the long form, and `FileSystemObject.GetFile().Path` can throw
  (a silent fallback hides it). Assert identity instead - leaf name, a per-run
  id, and equal SHA-256.
- The `.ico` the app embeds lives in `OUT_DIR`; `cargo run --example emit_icon --
  target\AirQlality.ico` writes the same bytes, and
  `ExtractAssociatedIcon(..).ToBitmap()` + quadrant pixels proves Inno picked it
  up. The setup wizard is a child `.tmp` process (window class `TWizardForm`), so
  `app/tests/capture.ps1` cannot screenshot it - use `EnumWindows` by class+title
  (`experiments/EXP29/capture-setup.ps1`).
- MinGW on this box is `%LOCALAPPDATA%\mingw64\bin`, not `C:\mingw64\bin`; a
  background shell does not inherit it and a fresh link dies with
  `ld: cannot find -lktmw32`.
