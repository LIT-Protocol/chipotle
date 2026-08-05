# Attribution

The `differential-review` skill (and the bundled `adversarial-modeler` agent at
`.claude/agents/adversarial-modeler.md`) is vendored from the Trail of Bits
Claude Code skills marketplace:

- Source: https://github.com/trailofbits/skills (`plugins/differential-review`)
- Author: Omar Inuwa, Trail of Bits (opensource@trailofbits.com)
- Version vendored: 1.1.1
- License: Creative Commons Attribution-ShareAlike 4.0 International (CC BY-SA 4.0)
  — https://creativecommons.org/licenses/by-sa/4.0/

It is vendored (rather than fetched at CI time) so the exact reviewed content is
pinned in-repo, reviewable in PRs, and carries no CI-time supply-chain dependency.

To update: re-copy from the upstream marketplace and bump the version above in the
same PR so the change is reviewable.
