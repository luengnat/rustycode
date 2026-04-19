import re
paths = ["crates/rustycode-tui/src/app/renderer.rs", "crates/rustycode-tui/src/app/event_loop.rs"]
for path in paths:
    with open(path, "r") as fp:
        content = fp.read()
    content = content.replace("crate::app::render::search::", "crate::app::renderer::")
    content = content.replace("crate::app::render::tools::", "crate::app::renderer::")
    content = content.replace("crate::app::render::selectors::", "crate::app::renderer::")
    with open(path, "w") as fp:
        fp.write(content)
print("done")
