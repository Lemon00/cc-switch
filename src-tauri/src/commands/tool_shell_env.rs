//! Parse login-shell environment records without treating multiline values as new variables.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct LoginShellEnv {
    pub(super) path: Option<String>,
    pub(super) vars: Vec<(String, String)>,
}

pub(super) fn parse_login_shell_env(raw: &str) -> Option<LoginShellEnv> {
    let (_, records) = raw.split_once("\0__CC_SWITCH_ENV__\0")?;
    if !records.ends_with('\0') {
        return None;
    }
    let mut env = LoginShellEnv::default();
    for record in records.split_terminator('\0') {
        let Some((key, value)) = record.split_once('=') else {
            continue;
        };
        if key == "PATH" {
            env.path = Some(value.to_string());
        } else if matches!(
            key,
            "PNPM_HOME"
                | "NVM_DIR"
                | "NVM_BIN"
                | "FNM_DIR"
                | "FNM_MULTISHELL_PATH"
                | "VOLTA_HOME"
                | "BUN_INSTALL"
                | "MISE_DATA_DIR"
                | "MISE_CONFIG_DIR"
                | "VITE_PLUS_HOME"
                | "VITE_PLUS_BIN_DIR"
        ) {
            // Copy manager configuration only; credentials, shell internals and
            // application-specific overrides remain those of the parent process.
            env.vars.push((key.to_string(), value.to_string()));
        }
    }
    Some(env)
}

pub(super) fn apply_login_shell_vars(cmd: &mut std::process::Command, vars: &[(String, String)]) {
    cmd.envs(vars.iter().map(|(key, value)| (key, value)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiline_values_and_shell_noise_cannot_replace_path() {
        let env = parse_login_shell_env(
            "PATH=/noise\n\0__CC_SWITCH_ENV__\0SCRIPT=line1\nPATH=/injected\0PATH=/usr/bin:/bin\0PNPM_HOME=/home/a b/pnpm\0API_KEY=secret\0HOME=/other\0",
        ).unwrap();
        assert_eq!(env.path.as_deref(), Some("/usr/bin:/bin"));
        assert_eq!(
            env.vars,
            vec![("PNPM_HOME".into(), "/home/a b/pnpm".into())]
        );
        let mut command = std::process::Command::new("fixture");
        command.env("PATH", "merged-path");
        apply_login_shell_vars(&mut command, &env.vars);
        assert!(command.get_envs().any(
            |(key, value)| key == "PATH" && value == Some(std::ffi::OsStr::new("merged-path"))
        ));
    }

    #[test]
    fn incomplete_or_unmarked_environment_is_rejected() {
        assert!(parse_login_shell_env("PATH=/usr/bin\0").is_none());
        assert!(parse_login_shell_env("\0__CC_SWITCH_ENV__\0PATH=/truncated").is_none());
    }
}
