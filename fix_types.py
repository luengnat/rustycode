import os

path = "crates/rustycode-tui/src/app/render/messages.rs"
with open(path, "r") as fp:
    content = fp.read()

content = content.replace(".and_then(|n| n.to_str())", ".and_then(|n: &std::ffi::OsStr| n.to_str())")
content = content.replace(".fold(0u8, |a, b| a.wrapping_add(b))", ".fold(0u8, |a: u8, b: u8| a.wrapping_add(b))")
content = content.replace("tui.apply_search_highlighting", "crate::app::render::search::apply_search_highlighting")

with open(path, "w") as fp:
    fp.write(content)

print("done")
