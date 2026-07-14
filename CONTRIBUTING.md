## Commit message guide

* Keep commits **small, focused, and reviewable**. One logical change per commit.
* **Split long AI runs** into multiple commits (e.g. refactor → implementation → tests → docs). Do not bundle unrelated work.
* Use this format:

```text
<concise summary>

- What changed
- Why it changed
- Important context (breaking change, migration, tests, risks, etc.)
```

* The summary should describe the **outcome**, not the process.
* Bullet points should highlight only the important details—don't narrate the diff.
* Always include the **reasoning** for non-trivial changes.
* If the current changes span multiple concerns, **propose multiple commits instead of one large commit**.
