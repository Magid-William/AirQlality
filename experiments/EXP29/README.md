# EXP29 - a per-user Windows installer next to the zip

**Status:** `inProg` (started 2026-10-22).

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
  exit 0, the running copy closed, a portable copy left alone). The setup is
  **10.1 MiB** from the 32.6 MiB exe (lzma2/max), vs a 33 MB zip. Suites: core
  64/64, app 82/82, windowed E2E 33/33 in 276 s - no product code changed, so no
  re-baseline.
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
_Pending._

## Learnings
- A hosted runner's `%TEMP%` is an **8.3 short path** (`C:\Users\RUNNER~1\...`)
  while shortcuts and the registry keep the long one: never compare raw path
  strings, and don't trust `FileSystemObject.GetFile().Path` either (it threw on
  the runner and the silent fallback hid it). The proof compares what the target
  *is*: leaf `airqlality.exe`, inside this run's scratch id, and a SHA-256 equal to
  the installed exe. This was the only failing assertion in the first two `v0.3.0`
  runs - the installer itself was correct both times.
- Inno Setup is **not** on the `windows-2025` runner image (it was on 2022):
  `choco install innosetup --no-progress -y` then use
  `C:\Program Files (x86)\Inno Setup 6\ISCC.exe`. `winget install
  JRSoftware.InnoSetup` on a dev box lands in
  `%LOCALAPPDATA%\Programs\Inno Setup 6\` instead - search both.
- `ISCC.exe /DAppVersion=<ver> /DSourceExe=<exe> /DSourceIcon=<ico> <script>.iss`
  - use when building the setup; `#ifndef` defaults in the script keep a bare
  `cargo build --release` tree working.
- `UninstallDisplayName` is what Add/Remove Programs shows; without it Inno uses
  `AppVerName` (`AirQlality 0.2.0-test`) and the ARP key is
  `HKCU\...\Uninstall\{AppId}_is1`. Enumerate by `DisplayName`, not by path.
- `PrivilegesRequired=lowest` really means no UAC: `elevated=False`, install to a
  scratch dir, uninstall, all silent (`/VERYSILENT /SUPPRESSMSGBOXES
  /NORESTART /DIR=...`).
- Silent installs skip `unchecked` tasks: the opt-in desktop icon stays absent
  under `/VERYSILENT` (three consecutive proof runs), so the silent proof does
  not create desktop clutter.
- The Inno uninstaller copies itself to `%TEMP%` and **returns immediately**:
  `Start-Process -Wait` proves nothing - poll for the exe to disappear.
- The setup's icon must be a file, and the EXP14 `.ico` only exists in
  `OUT_DIR`: `cargo run --example emit_icon -- target\AirQlality.ico` writes the
  same bytes; check it landed with `ExtractAssociatedIcon(...).ToBitmap()` and
  sample the four quadrant pixels.
- `app/tests/capture.ps1` cannot screenshot the installer: the wizard lives in a
  child `.tmp` process (the started exe is only the extractor) with window class
  `TWizardForm`, so find it with `EnumWindows` + class + title
  (`experiments/EXP29/capture-setup.ps1`).
- Silent upgrade over a **running** tray app failed with **exit 5** until setup
  stopped it itself: `CloseApplications=yes` (Restart Manager) cannot close an
  app that treats `WM_CLOSE` as "hide to the tray". Fix: `[Files]` entry with
  `Flags: dontcopy` + `ExtractTemporaryFile` + `Exec(powershell -File stop-app.ps1
  -Path "{app}\airqlality.exe")` in `PrepareToInstall`, matching on the process
  `Path` so a portable copy elsewhere is untouched. Use when packaging any
  tray/hide-on-close app.
- On this box MinGW is `%LOCALAPPDATA%\mingw64\bin`, **not** `C:\mingw64\bin`; a
  background shell did not inherit it and a fresh link died with
  `ld: cannot find -lktmw32`. Set it explicitly:
  `$env:PATH = "$env:LOCALAPPDATA\mingw64\bin;$env:PATH"` before
  `cargo build`/`test`.

## Still open when the session stopped (2026-10-22)
- One unproven one-off: the very first proof run found a desktop shortcut even
  though the task is `unchecked`; every later run (many installs, including
  upgrades) never created one. Suspected stale artifact, not reproduced.
- The published asset itself: `dist` exe + setup are built and proven on CI, so
  the first tagged release after this branch is the real end-to-end check.
- Restart after an upgrade: setup stops the running widget and does not start it
  again unless the user asks on the finish page (silent installs stay silent).
