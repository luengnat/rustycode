import os, re

files = ["search.rs", "selectors.rs", "tools.rs", "input.rs", "status.rs", "messages.rs"]

for f in files:
    path = f"crates/rustycode-tui/src/app/render/{f}"
    with open(path, "r") as fp:
        content = fp.read()
    
    # 1. Strip the `impl crate::app::renderer::PolishedRenderer {` block wrapping from search, selectors, tools
    if f in ["search.rs", "selectors.rs", "tools.rs"]:
        content = content.replace("impl crate::app::renderer::PolishedRenderer {\n", "")
        # Remove the last closing brace
        content = content.rsplit("}", 1)[0]
        # Remove `&self, ` from method signatures
        content = content.replace("pub fn render_", "pub fn render_")
        content = re.sub(r'pub fn (.*?)\(&self, tui: &mut', r'pub fn \1(tui: &mut', content)
        content = re.sub(r'pub fn (.*?)\(&mut self, tui: &mut', r'pub fn \1(tui: &mut', content)
        content = re.sub(r' fn (.*?)\(&self, tui: &mut', r' fn \1(tui: &mut', content)

    # 2. Fix the `self.` -> `tui.` that was missing in `renderer.rs`
    # Wait, renderer.rs needs to call `tui.render_input` -> `PolishedRenderer::render_input(self, tui, frame, area)`
    
    with open(path, "w") as fp:
        fp.write(content)

print("done")
