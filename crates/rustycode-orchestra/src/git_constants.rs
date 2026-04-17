//! Shared git constants used across git-service and native-git-bridge

use std::collections::HashMap;

/// Env overlay that suppresses interactive git credential prompts and git-svn noise
///
/// Returns a HashMap of environment variables that disable:
/// - Interactive terminal prompts (GIT_TERMINAL_PROMPT=0)
/// - SSH askpass prompts (GIT_ASKPASS="")
/// - Git-svn identification noise (GIT_SVN_ID="")
///
/// # Example
/// ```
/// use rustycode_orchestra::git_constants::*;
///
/// let env = git_no_prompt_env();
/// assert_eq!(env.get("GIT_TERMINAL_PROMPT"), Some(&"0".to_string()));
/// ```
pub fn git_no_prompt_env() -> HashMap<String, String> {
    let mut env = HashMap::new();
    env.insert("GIT_TERMINAL_PROMPT".to_string(), "0".to_string());
    env.insert("GIT_ASKPASS".to_string(), "".to_string());
    env.insert("GIT_SVN_ID".to_string(), "".to_string());
    env
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_no_prompt_env() {
        let env = git_no_prompt_env();

        assert_eq!(env.get("GIT_TERMINAL_PROMPT"), Some(&"0".to_string()));
        assert_eq!(env.get("GIT_ASKPASS"), Some(&"".to_string()));
        assert_eq!(env.get("GIT_SVN_ID"), Some(&"".to_string()));
        assert_eq!(env.len(), 3);
    }

    #[test]
    fn test_git_no_prompt_env_terminal_prompt() {
        let env = git_no_prompt_env();
        assert_eq!(env["GIT_TERMINAL_PROMPT"], "0");
    }

    #[test]
    fn test_git_no_prompt_env_askpass() {
        let env = git_no_prompt_env();
        assert_eq!(env["GIT_ASKPASS"], "");
    }

    #[test]
    fn test_git_no_prompt_env_svn_id() {
        let env = git_no_prompt_env();
        assert_eq!(env["GIT_SVN_ID"], "");
    }
}
