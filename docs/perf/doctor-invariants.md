# Doctor scope and retained entries

Doctor reports errors for broken links, corrupt registries or journals, and
pending history events. A skill link may resolve anywhere inside the home
or a selected project. A target outside every scope is a warning.

Retained trash, quarantine entries, and orphaned parked or fork entries warn
after 30 days. `RETAINED_ENTRY_MAX_AGE_DAYS` defines the age once. A recorded
creation time takes precedence over mtime. Mtime applies only when no record
exists. An invalid recorded time gets a warning; touching an old entry does
not reset its age. The count cap remains 20 entries per skill. Active parked
skills and active fork bases are excluded at any age. Doctor never deletes.

These link and retention rules are lead defaults for the owner to confirm or
change. They do not authorize cleanup.

Core fixtures cover all four link cases, every agent root and the shared
root, corrupt files, pending plans, retained ages, and active references.
A host integration test checks real SQLite pending events without changing
status. CLI, MCP stdio, and desktop adapter tests cover the same report.
The CLI's human output labels each violation as warning or error.
