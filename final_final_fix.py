import re

# Fix renderer.rs & event_loop.rs module paths
for f in ["crates/rustycode-tui/src/app/renderer.rs", "crates/rustycode-tui/src/app/event_loop.rs"]:
    with open(f, "r") as fp: c = fp.read()
    c = c.replace("crate::app::render::tools::", "crate::app::renderer::")
    c = c.replace("crate::app::render::selectors::", "crate::app::renderer::")
    # Also I forgot to pass `tui` in renderer.rs: `crate::app::renderer::render_tool_panel(frame, chunks[2]);`
    c = c.replace("crate::app::renderer::render_tool_panel(frame", "crate::app::renderer::render_tool_panel(tui, frame")
    with open(f, "w") as fp: fp.write(c)

# Fix messages.rs multi-line self
path = "crates/rustycode-tui/src/app/render/messages.rs"
with open(path, "r") as fp: c = fp.read()
c = re.sub(r'\bself\s*\.\s*services', 'tui.services', c)
c = re.sub(r'\bself\s*\.\s*message_renderer', 'tui.message_renderer', c)
c = re.sub(r'\bself\s*\.\s*messages', 'tui.messages', c)
c = re.sub(r'\bself\s*\.\s*selected_message', 'tui.selected_message', c)
with open(path, "w") as fp: fp.write(c)

# Fix status.rs multi-line self
path = "crates/rustycode-tui/src/app/render/status.rs"
with open(path, "r") as fp: c = fp.read()
c = re.sub(r'\bself\s*\.\s*workspace_tasks', 'tui.workspace_tasks', c)
with open(path, "w") as fp: fp.write(c)

# Fix tools.rs clone issue: E0502: cannot borrow *tui as mutable because it is also borrowed as immutable
path = "crates/rustycode-tui/src/app/render/tools.rs"
with open(path, "r") as fp: c = fp.read()
# Change: `let tool = &tui.tool_panel_history[selected_idx]; render_tool_result_detail(tui, frame, area, tool);`
# To: `let tool = tui.tool_panel_history[selected_idx].clone(); render_tool_result_detail(tui, frame, area, &tool);`
c = c.replace("let tool = &tui.tool_panel_history[selected_idx];\n                    render_tool_result_detail(tui, frame, area, tool);", "let tool = tui.tool_panel_history[selected_idx].clone();\n                    render_tool_result_detail(tui, frame, area, &tool);")
with open(path, "w") as fp: fp.write(c)

print("done")
