# Project file v4 for Turn provenance

Turn provenance is introduced in Project-file version 4, with explicit migration from version 3 and earlier files. Although provenance is optional on an assistant GraphNode, retaining version 3 would let older ThoughtTree builds load the unknown field and silently erase it on their next save; older builds must instead reject version 4 so evidence cannot be lost without warning.
