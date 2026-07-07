---
status: idea
priority: medium
tags: [feature]
created: 2026-03-31
---

# Claude Code permission config

Three parts:

1. **Forward permission requests to UI** — When Claude Code (via ACP) hits a permission gate, surface the request in the ThoughtTree UI so the user can approve or deny it interactively, rather than it silently blocking.

2. **Configurable permissions in project settings** — Let users pre-configure permission policies (e.g. allow file reads, deny network access) per project within ThoughtTree's settings, so common decisions don't require repeated manual approval.

3. **Subtle notifications for denied actions** — When Claude skips an action due to missing permissions, show a visually pleasing, non-intrusive notification in the UI so the user is aware something was skipped without being disrupted.
