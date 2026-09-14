# Project file v5 for file nodes

File nodes (role `file`, a Vault-relative file reference) are introduced in Project-file version 5. Loading version 4 is a no-op migration: the shape is unchanged, and older files simply contain no file nodes. Retaining version 4 would let older ThoughtTree builds drop the unknown `file` role on load and silently erase those nodes and their edges on the next save; older builds must instead reject version 5 so linked files cannot vanish without warning (same reasoning as ADR 0006).
