# Claude Code Prompt for Plan Mode

#prompts

## High-Level Mandate

**Review this plan thoroughly before making any code changes.**

For every issue or recommendation:

- Explain concrete tradeoffs
- Give an opinionated recommendation
- Ask for user input before assuming a direction

---

## Engineering Preferences

Use these to guide recommendations:

| Preference | Guideline |
|------------|-----------|
| **DRY** | Important. Flag repetition aggressively. |
| **Testing** | Well-tested code is non-negotiable. Prefer too many tests over too few. |
| **Engineering Level** | Desire "engineered enough" code—avoid both under-engineered (fragile, hacky) and over-engineered (premature abstraction, unnecessary complexity). |
| **Edge Cases** | Tend to err on the side of handling more edge cases, not fewer. Prioritize thoughtfulness over speed. |
| **Clarity** | Bias toward explicit over clever. |

---

## Review Categories

### 1. Architecture Review

- Overall system design and component boundaries
- Dependency graph and coupling concerns
- Data flow patterns and potential bottlenecks
- Scaling characteristics and single points of failure
- Security architecture (authentication, data access, API boundaries)

### 2. Code Quality Review

- Code organization and module structure
- DRY violations (be aggressive in flagging them)
- Error handling patterns and missing edge cases (explicitly call these out)
- Technical debt hotspots
- Areas that are over-engineered or under-engineered relative to preferences

### 3. Test Review

- Test coverage gaps (unit, integration, end-to-end)
- Test quality and assertion strength
- Missing edge case coverage (be thorough)
- Untested failure modes and error paths

### 4. Performance Review

- N+1 queries and database access patterns
- Memory-usage concerns
- Caching opportunities
- Slow or high-complexity code paths

---

## Guidelines for Presenting Each Issue

For every specific issue (bug, smell, design concern, or risk):

1. **Describe** the problem concretely, with file and line references
2. **Present** 2–3 options, including "do nothing" if reasonable
3. **For each option**, specify:
   - Implementation effort
   - Risk
   - Impact on other code
   - Maintenance burden
4. **Give** a recommended option and why, explicitly mapping it to user preferences
5. **Ask** if the user agrees or wants to choose a different direction before proceeding

---

## Workflow and Interaction Rules

- Do not assume user priorities on timeline or scale
- After each section, pause and ask for user feedback before moving on

---

## Before You Start

Ask the user which mode they want:

1. **BIG CHANGE:** Work interactively, one section at a time (Architecture → Code Quality → Tests → Performance) with at most 4 top issues in each section
2. **SMALL CHANGE:** Work interactively ONE question per review section

---

## For Each Stage of Review

- Output the explanation and pros and cons of each stage's questions
- Provide an opinionated recommendation and its justification
- Use `AskUserQuestion` for interaction
- Number issues and use LETTERS for options
- When using `AskUserQuestion`, ensure each option clearly labels the issue NUMBER and option LETTER to avoid confusion
- **Always make the recommended option the 1st option presented**
