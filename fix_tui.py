import re
files = ["messages.rs", "status.rs", "input.rs"]
for f in files:
    path = f"crates/rustycode-tui/src/app/render/{f}"
    with open(path, "r") as fp:
        content = fp.read()
    
    # Global replace self. with tui. except for self.state
    # Because we are in PolishedRenderer, and we want to access TUI fields.
    # Wait, if we use `tui.` for everything, does TUI have all those fields?
    # Yes! TUI is the god struct.
    
    # Temporarily hide self.state
    content = content.replace("self.state", "SELF_STATE_TEMP")
    
    # Replace all self.
    content = re.sub(r'\bself\.', 'tui.', content)
    
    # Restore self.state
    content = content.replace("SELF_STATE_TEMP", "self.state")
    
    with open(path, "w") as fp:
        fp.write(content)

print("done")
