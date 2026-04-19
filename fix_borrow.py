import re

# Fix render_compaction_preview calls
path = "crates/rustycode-tui/src/app/renderer.rs"
with open(path, "r") as fp: c = fp.read()
c = c.replace("crate::app::renderer::render_compaction_preview(tui, frame, size);", "tui.render_compaction_preview(frame, size);")
with open(path, "w") as fp: fp.write(c)

path = "crates/rustycode-tui/src/app/event_loop.rs"
with open(path, "r") as fp: c = fp.read()
c = c.replace("crate::app::renderer::render_compaction_preview(self, frame, size);", "self.render_compaction_preview(frame, size);")
with open(path, "w") as fp: fp.write(c)

# Fix apply_search_highlighting mutability & signature
path = "crates/rustycode-tui/src/app/render/selectors.rs"
with open(path, "r") as fp: c = fp.read()
c = c.replace("pub fn apply_search_highlighting(\n        tui: &mut crate::app::event_loop::TUI,", "pub fn apply_search_highlighting(\n        tui: &crate::app::event_loop::TUI,")
with open(path, "w") as fp: fp.write(c)

print("done")
