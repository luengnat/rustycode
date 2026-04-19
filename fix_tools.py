import os
path = "crates/rustycode-tui/src/app/render/tools.rs"
with open(path, "r") as fp:
    c = fp.read()
c = c.replace("tui.render_tool_result_detail(frame, area, tool);", "render_tool_result_detail(tui, frame, area, tool);")
c = c.replace("fn render_tool_result_detail(\n        \n        frame:", "fn render_tool_result_detail(\n        tui: &mut crate::app::event_loop::TUI,\n        frame:")
with open(path, "w") as fp:
    fp.write(c)
print("done")
