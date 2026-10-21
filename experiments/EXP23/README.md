# EXP23 - Publish: scrub, license, squash, release

**Status:** `inProg`.

## Summary
Make the repository safe and fit to publish: remove local paths and other
identifying traces, add the MIT license plus the third-party data notices, add a
root README, squash 62 commits and 23 branches into one snapshot, and publish it
with `gh` as a public GitHub repository.

## Hypothesis
A repository prepared this way is publishable without leaking anything: after
EXP23 the history is one commit, the tree carries `LICENSE`, `NOTICE` and
`README.md`, a fresh clone builds and passes the tests, and `gh repo view`
reports a public `main` with nothing under the user's home directory, no email
in the commit metadata beyond the chosen identity, and no API key material.

**Falsified if:**
- any tracked file or commit contains a local absolute path, the login name, a
  personal email other than the chosen commit identity, or key material;
- the squashed history still exposes the old objects (`git fsck --unreachable`
  or `git count-objects` shows the pre-squash packs);
- the published tree differs from the verified working tree (untracked or
  ignored files slipped in);
- the fresh clone fails `cargo test -p airqlality-core`.

## Methodology & blast radius
- **Scan first, then clean.** `git grep` over the whole history for keys
  (`api[_-]?key`, `appid=`, `X-API-Key`, 32-hex strings), identity
  (`Users\`, `Playground\`, `hotmail`, `Maged`, `Magid`) and the untracked
  `.openchamber/` screenshots. Findings: no key material ever landed (only
  `Cargo.lock` checksums, `"secret"`/`"k"` test fixtures and `appid=***`
  redactions); personal traces are `AGENTS.md` (workspace + MinGW paths with the
  login name), two `EXP12` JSON dumps with the absolute exe path, and the author
  email in all 62 commits.
- **Scrub.** `AGENTS.md` goes repo-relative and drops the login name; the EXP12
  dumps name `target\debug\airqlality.exe`. `.gitignore` gains the tool and
  secret files (`.openchamber/`, `config.json`, `location.txt`, `.env`,
  `*.local.json`) so nothing can be added by accident. The unreferenced
  third-party screenshot `EXP20-aqi-in-cairo.jpg` is dropped (the
  `prior-art-aqi-in.md` study that cites the site stays).
- **License.** MIT for the code (matching `Cargo.toml`), and `NOTICE` for the
  data the binary carries: GeoNames (CC BY 4.0), Open-Meteo / CAMS ENSEMBLE
  (CC BY 4.0), OpenAQ (CC BY 4.0), OpenWeatherMap terms, and GRIP4 (ODbL) whose
  derived `core/assets/noise_density.json` is distributed under ODbL while the
  code remains MIT. `core/assets/ATTRIBUTION.md` stays the detailed record.
- **README.** Root `README.md`: what the widget is, screenshots, build and run
  (MinGW on PATH), the optional source keys and where they live, licensing, and
  the experiment log link.
- **Squash.** `git bundle create <temp>\AirQlality-history.bundle --all` keeps
  the 62-commit history restorable outside the repo. Then an orphan commit of
  the final tree, `main` force-renamed onto it, every `EXP*` branch deleted,
  `reflog expire --expire=now --all` and `gc --prune=now`.
- **Publish.** `gh repo create Magid-William/AirQlality --public --source .
  --remote origin --push`, description and topics, then a fresh clone into temp
  to re-run `cargo test -p airqlality-core` on exactly what the public sees.
- **May touch:** `experiments/EXP23/`, `experiments-overview.md`, `.gitignore`,
  `AGENTS.md`, the two EXP12 JSON dumps, new `README.md` / `LICENSE` / `NOTICE`,
  the deleted jpg, all git refs, the new GitHub repo.
- **Off-limits:** app and core source logic, the tests, and the other
  experiments' content.

## Open questions (known unknowns)
- [x] Owner, name and visibility - `Magid-William/AirQlality`, public.
- [x] License - MIT only, matching the workspace `license` field.
- [x] Commit identity - the one git is already configured with (name + email),
      reused for the single squashed commit.
- [x] Keep the evidence screenshots - yes; the experiments are the story.
- [x] Keep `AGENTS.md` and `opencode.json` - yes, sanitized.
- [x] `EXP09/15/17/18/19` still say `inProg` - left untouched; only the user
      declares final status.
- [x] Windowed E2E - skipped this once: no app code changes, so core and app
      unit tests are the evidence, and the run would flash windows for minutes.

## Conclusion (findings)
_Pending._

## Learnings
- `git grep -n -I -E "..." -- .` - scan the tree only; add `git log --all -p`
  for history, but expect `Cargo.lock` checksum noise.
- `gh api user --jq .login` - the login is `Magid-William`; `gh auth status`
  labels the account `Maged-William`, so confirm the login before
  `gh repo create <owner>/<repo>` (a wrong owner is a bare `HTTP 404`).
- `git bundle create <temp>\AirQlality-history.bundle --all` + `git bundle
  verify <file>` - always, before squashing away history.
- `git checkout --orphan`, `git add -A`, one commit, `git branch -M main`,
  delete every old branch **and tag**, `git reflog expire --expire=now --all`,
  `git gc --prune=now` - leaves one commit and a single pack (3.74 MiB);
  a leftover tag silently keeps the old objects alive and pushes them.
- `git rm --cached <file>` stages a deletion that the *next* `git commit`
  picks up even if you only listed other paths - commit by path list, or
  expect the deletion in an earlier commit.
- PowerShell has no heredoc: write the commit message to a file and use
  `git commit -F <file>`.
- The app icon for the README is the **256 px frame of the EXP14 `.ico`** (BMP
  frames, not PNG): parse the ICO directory, then `struct` + `zlib` write a
  32-bit RGBA PNG - `System.Drawing.Icon` only ever handed back the 48 px frame.
- Verify from a fresh clone: `git clone <url> <temp>` then
  `cargo test -p airqlality-core --offline` (64/64) proves the published tree,
  not just the local one.
