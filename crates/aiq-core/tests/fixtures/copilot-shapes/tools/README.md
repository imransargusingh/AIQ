# Refreshing the Copilot shape fixtures

The fixtures one folder up are real Copilot session logs reduced to their shape (see the header of
`crates/aiq-core/tests/copilot_shapes.rs`). To refresh them after Copilot changes its log:

1. On the machine with the logs: `python3 shape.py <copilot home>/session-state <out>`. This picks up to six
   sessions (steering, sub-agents, queued and idle messages) and replaces every string outside a short allowlist.
2. `python3 trim.py <out> <fixtures>` keeps the event types AIQ reads, with a tighter allowlist, at most 120 rows each.
3. Before committing, list every string value left in the files. Only event types, `delivery`, `role`, `status`,
   `kind`, the allowlisted tool names, `<text>`, fake `id-…` ids and timestamps may remain.
4. Recount the expected numbers in `copilot_shapes.rs` from the new files. Never take them from the code.

Both scripts use only the standard library and never write to the source folder.
