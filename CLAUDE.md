# gstack

Use the `/browse` skill from gstack for **all web browsing**. Never use `mcp__claude-in-chrome__*` tools — they are slow and unreliable. The gstack browse binary is a fast headless Chromium that persists state between calls.

## Available skills

| Skill | Description |
|-------|-------------|
| `/office-hours` | YC Office Hours — startup forcing questions or design brainstorm |
| `/plan-ceo-review` | CEO/founder plan review — challenges premises, expands scope |
| `/plan-eng-review` | Eng manager plan review — architecture, data flow, edge cases |
| `/plan-design-review` | Designer's eye plan review with 0-10 ratings |
| `/design-consultation` | Propose full design systems, generate DESIGN.md |
| `/review` | Pre-landing PR review — SQL safety, LLM trust, structural issues |
| `/ship` | Full ship workflow: merge, test, bump version, PR |
| `/land-and-deploy` | Merge PR → wait for CI → verify prod health |
| `/canary` | Post-deploy canary monitoring — watch for errors/regressions |
| `/benchmark` | Performance regression detection (Core Web Vitals, load times) |
| `/browse` | Headless browser: navigate, interact, screenshot, QA test |
| `/qa` | QA test + fix bugs iteratively with atomic commits |
| `/qa-only` | QA report only — never fixes, just documents with health scores |
| `/design-review` | Designer's eye QA — visual inconsistency, spacing, hierarchy |
| `/setup-browser-cookies` | Import real Chromium cookies into headless browse session |
| `/setup-deploy` | Configure deployment settings for /land-and-deploy |
| `/retro` | Weekly engineering retrospective with trend tracking |
| `/investigate` | Systematic debugging — root cause required before fixing |
| `/document-release` | Post-ship doc updates (README, CHANGELOG, CLAUDE.md) |
| `/codex` | Independent second-opinion code review |
| `/cso` | Chief Security Officer audit — secrets, supply chain, OWASP, STRIDE |
| `/autoplan` | Auto-runs CEO + design + eng reviews sequentially |
| `/careful` | Warns before destructive commands (rm -rf, DROP TABLE, etc.) |
| `/freeze` | Restrict file edits to a specific directory |
| `/guard` | Full safety mode: destructive command warnings + directory restrictions |
| `/unfreeze` | Clear the freeze set by /freeze |
| `/gstack-upgrade` | Upgrade gstack to latest version |

## Troubleshooting

If gstack skills aren't working (browse binary missing, skills not loading):

```bash
cd .claude/skills/gstack && ./setup
```

This rebuilds the browse binary and re-registers all skills.
