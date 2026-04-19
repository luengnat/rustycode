import re

files_impl = ["input.rs", "status.rs", "messages.rs"]
for f in files_impl:
    path = f"crates/rustycode-tui/src/app/render/{f}"
    with open(path, "r") as fp:
        c = fp.read()
    # Any 'self.[a-z]' that is NOT self.state and NOT self.render_... becomes tui.[a-z]
    def repl_impl(match):
        field = match.group(1)
        if field == "state" or field.startswith("render_"):
            return f"self.{field}"
        return f"tui.{field}"
    c = re.sub(r'self\.([a-zA-Z_0-9]+)', repl_impl, c)
    with open(path, "w") as fp:
        fp.write(c)

files_free = ["tools.rs", "selectors.rs", "search.rs"]
for f in files_free:
    path = f"crates/rustycode-tui/src/app/render/{f}"
    with open(path, "r") as fp:
        c = fp.read()
    c = c.replace("&self,", "")
    c = c.replace("self.", "tui.")
    with open(path, "w") as fp:
        fp.write(c)

# Fix apply_search_highlighting call in messages.rs
path = "crates/rustycode-tui/src/app/render/messages.rs"
with open(path, "r") as fp:
    c = fp.read()
c = c.replace("crate::app::renderer::apply_search_highlighting(&lines, msg_idx)", "crate::app::renderer::apply_search_highlighting(tui, &lines, msg_idx)")
c = c.replace("crate::app::renderer::apply_search_highlighting(&lines, /* &[ratatui::prelude::Line<'_>] */, msg_idx)", "crate::app::renderer::apply_search_highlighting(tui, &lines, msg_idx)")
with open(path, "w") as fp:
    fp.write(c)

print("done")
