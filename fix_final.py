import re

# Fix tools.rs
path = "crates/rustycode-tui/src/app/render/tools.rs"
with open(path, "r") as fp:
    c = fp.read()
c = c.replace("= self\n", "= tui\n")
c = c.replace("= self\r\n", "= tui\r\n")
c = c.replace("if self.tool_result_show_full", "if tui.tool_result_show_full")
c = c.replace("if tui.tool_result_show_full", "if tui.tool_result_show_full")
c = re.sub(r'\bself\.tool_result', 'tui.tool_result', c)
c = re.sub(r'\bself\.message_renderer', 'tui.message_renderer', c)
c = re.sub(r'\bself\.viewport_height', 'tui.viewport_height', c)
with open(path, "w") as fp:
    fp.write(c)

# Fix messages.rs
path = "crates/rustycode-tui/src/app/render/messages.rs"
with open(path, "r") as fp:
    c = fp.read()
c = re.sub(r'\bself\.services', 'tui.services', c)
c = re.sub(r'\bself\.message_renderer', 'tui.message_renderer', c)
c = re.sub(r'\bself\.messages', 'tui.messages', c)
c = re.sub(r'\bself\.selected_message', 'tui.selected_message', c)
with open(path, "w") as fp:
    fp.write(c)

# Fix status.rs
path = "crates/rustycode-tui/src/app/render/status.rs"
with open(path, "r") as fp:
    c = fp.read()
c = re.sub(r'\bself\.workspace_tasks', 'tui.workspace_tasks', c)
with open(path, "w") as fp:
    fp.write(c)

# Fix selectors.rs
path = "crates/rustycode-tui/src/app/render/selectors.rs"
with open(path, "r") as fp:
    c = fp.read()
c = c.replace("= self\n", "= tui\n")
c = c.replace("= self\r\n", "= tui\r\n")
with open(path, "w") as fp:
    fp.write(c)

# Fix event_loop.rs
path = "crates/rustycode-tui/src/app/event_loop.rs"
with open(path, "r") as fp:
    c = fp.read()
c = c.replace("crate::app::renderer::render_compaction_preview", "crate::app::render::selectors::render_compaction_preview")
with open(path, "w") as fp:
    fp.write(c)

# Fix renderer.rs
path = "crates/rustycode-tui/src/app/renderer.rs"
with open(path, "r") as fp:
    c = fp.read()
c = c.replace("tui.render_tool_panel", "crate::app::render::tools::render_tool_panel")
with open(path, "w") as fp:
    fp.write(c)

print("done")
