# AGENTS.md

## What this project about
In this empty folder, im starting a new project, it's an app that simply does one thing very well, it suggests when is it safe to open the window for fresh air, i live in Cairo Egypt, and air quality is a huge question mark.
I want to rely on opensource data and free resources to gather the data, and display on a timeline when it's okay to open the window.

The app should be made with https://gpui.rs, on windows as the target.

I imagine the wedget to be like this: @UI.md

## Bench notes
- It's a **sin** to explore other folders outside this repository.
- App builds need the MinGW toolchain on PATH: prepend `<mingw64>\bin` (for
  example `C:\mingw64\bin`) to `PATH` before `cargo build`/`test`,
  else `psm`'s build script (`gcc.exe`) and linking (`-lktmw32`) fail.


## Experiment workflow
Experiments are numbered EXP01, EXP02, ... One git branch per experiment,
one `experiments/EXP0N/README.md` per experiment. No change lands without an
experiment that states why.

| Step | Trigger | Action | Overview status |
|---|---|---|---|
| 1 Number   | user declares a new experiment | run `git log --oneline --branches`, read `experiments-overview.md`, tree `experiments/`; pick the next free number -> branch `EXP0N` | - |
| 2 Plan     | "start EXP0N ..." | plan mode: formalize + research viability; output Summary / Hypothesis / Methodology & blast radius / Open questions. Each open question carries a proposed default. No files written in plan mode. | - |
| 3 Record   | plan approved (build mode) | create the branch, write `experiments/EXP0N/README.md` from the template below, add an overview row | `todo` |
| 4 Run      | work begins | do the work; keep Learnings current as discoveries happen | `inProg` |
| 5 Conclude | user says "conclude EXP0N" | write the Conclusion/Findings, compress the Learnings, commit all changes on the branch, update the overview row | user-declared |

## Status
`todo | inProg | success | failed | abandoned`

Only the USER declares `success`, `failed` or `abandoned`. The agent recommends
and waits; it never declares a status on its own. Findings that contradict the
goal are a candidate for
`failed` -> propose it, do not set it.

`abandoned` is set only when the user says to abandon the experiment.

## Rules
- One experiment = one hypothesis. Make the blast radius explicit before starting.
- Run the windowed E2E suite (`cargo test -p airqlality --test e2e_widget`) **once,
  at the end of the experiment**. Every test spawns the real widget and
  screenshots its window, so a run flashes windows at the user for minutes.
  While iterating, use `cargo test -p airqlality-core` and
  `cargo test -p airqlality --bin airqlality` (no window), or a single E2E test
  by name when a pixel assertion is the thing under test.
- Branch per experiment, named after the EXP number (`EXP0N`).
- Plan mode is read-only: no branch, README or overview edits until the plan is approved.
- The overview is updated at every transition: plan-ready -> in progress -> final status.

## Open questions
- List them at plan time with a **proposed default** for each.
- If the user does not answer, adopt your proposed default, mark it explicitly
  as an assumption, and proceed. Never block waiting on an open question.
- If an assumption is later proven wrong, note it in Learnings and the Conclusion.

## Learnings (agent memory)
- Each experiment keeps a running `## Learnings` list: what was tried, the exact
  command that finally worked, and when to use it. Update it as you go so future
  sessions skip rediscovery instead of repeating it.
- On conclude, compress it to the smallest useful form.
- Durable, reusable facts only; never volatile per-run noise.
- At most ONE key learning may be promoted into AGENTS.md, and only after asking
  the user and getting the reason. Rare on purpose: keep AGENTS.md small.

## Experiment README template

```markdown
# EXP0N - <title>

## Summary
<goal for humans, < 250 chars>

## Hypothesis
<falsifiable prediction>

## Methodology & blast radius
- Method:
- May touch:
- Off-limits:

## Open questions (known unknowns)
- [ ] <question> — proposed default: <answer + why>

## Conclusion (findings)
_Pending._

## Learnings
- `<command that worked>` — use when <condition>
```
