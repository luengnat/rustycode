import os, re

files = ["input.rs", "status.rs", "messages.rs"]
for f in files:
    path = f"crates/rustycode-tui/src/app/render/{f}"
    with open(path, "r") as fp:
        content = fp.read()
    # Replace any leftover `self.` with `tui.` inside the PolishedRenderer methods, except self.state
    # Because `self` is `&PolishedRenderer`, the only valid field is `self.state`.
    # Wait, the error said `no field messages on type &PolishedRenderer`. So we MUST replace self.messages with tui.messages
    # Let's just blindly replace self. with tui. except for self.state and self.render
    lines = content.split('\n')
    for i in range(len(lines)):
        # Very simple targeted replacements based on compiler output
        lines[i] = lines[i].replace("self.services", "tui.services")
        lines[i] = lines[i].replace("self.message_renderer", "tui.message_renderer")
        lines[i] = lines[i].replace("self.messages", "tui.messages")
        lines[i] = lines[i].replace("self.selected_message", "tui.selected_message")
        lines[i] = lines[i].replace("self.workspace_tasks", "tui.workspace_tasks")
    with open(path, "w") as fp:
        fp.write('\n'.join(lines))

files = ["tools.rs", "selectors.rs", "search.rs"]
for f in files:
    path = f"crates/rustycode-tui/src/app/render/{f}"
    with open(path, "r") as fp:
        content = fp.read()
    lines = content.split('\n')
    for i in range(len(lines)):
        # Free functions, `self.` is totally invalid here.
        lines[i] = lines[i].replace("self.", "tui.")
    with open(path, "w") as fp:
        fp.write('\n'.join(lines))

# Also fix the import error in event_loop.rs
# `could not find selectors in render` -> because `selectors.rs` has NO pub mod in `render`!
