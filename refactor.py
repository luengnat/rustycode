import os, glob

files = [
    "input.rs", "status.rs", "messages.rs", "search.rs", "selectors.rs", "tools.rs"
]

for f in files:
    path = f"crates/rustycode-tui/src/app/render/{f}"
    with open(path, "r") as fp:
        content = fp.read()
    
    # Replace `impl TUI {`
    content = content.replace("impl TUI {", "impl crate::app::renderer::PolishedRenderer {")
    
    # Replace method signatures
    content = content.replace("&self, frame: &mut ratatui::Frame", "&self, tui: &mut crate::app::event_loop::TUI, frame: &mut ratatui::Frame")
    content = content.replace("&mut self, frame: &mut ratatui::Frame", "&self, tui: &mut crate::app::event_loop::TUI, frame: &mut ratatui::Frame")
    content = content.replace("&self, area", "&self, tui: &mut crate::app::event_loop::TUI, area")
    content = content.replace("&self, size", "&self, tui: &mut crate::app::event_loop::TUI, size")
    # For tools.rs
    content = content.replace("pub fn render_tool_panel(&self, frame: &mut ratatui::Frame,", "pub fn render_tool_panel(&self, tui: &mut crate::app::event_loop::TUI, frame: &mut ratatui::Frame,")
    content = content.replace("pub fn render_worker_panel(&self, frame: &mut ratatui::Frame,", "pub fn render_worker_panel(&self, tui: &mut crate::app::event_loop::TUI, frame: &mut ratatui::Frame,")

    # The magic: replace `self.` with `tui.` for all property accesses!
    # BUT we want to keep `self.state` if it exists. None of the old methods use `self.state` because TUI didn't have `state` field.
    content = content.replace("self.", "tui.")
    
    # Some things are pure functions inside the block
    content = content.replace("tui: &mut crate::app::event_loop::TUI, tui:", "tui: &mut crate::app::event_loop::TUI,")
    
    # And fix Brutalist renderer branches - we just delete them.
    # We will use multi_replace directly for brutalist blocks, but Python can help strip them.
    
    with open(path, "w") as fp:
        fp.write(content)

print("done")
