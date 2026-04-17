use anyhow::{anyhow, Result};
pub mod codec;
pub mod permission;
pub mod post_tool;
pub mod pre_tool;
pub mod rules;

pub fn process_hook(input_json: &str, hook_type: &str) -> Result<String> {
    let input = crate::codec::parse_input(input_json)?;
    let result = match hook_type {
        "pre-tool" => crate::pre_tool::evaluate(&input),
        "post-tool" => crate::post_tool::evaluate(&input),
        "permission" => crate::permission::evaluate(&input),
        _ => return Err(anyhow!("Unknown hook type")),
    };
    Ok(crate::codec::format_result_string(&result))
}
