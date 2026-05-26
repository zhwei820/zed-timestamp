use zed_extension_api::{self as zed, LanguageServerId, Result};

struct TimestampExtension;

impl zed::Extension for TimestampExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        _id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let path = worktree.which("timestamp-lsp").ok_or_else(|| {
            "`timestamp-lsp` binary not found on PATH. \
             Install with `cargo install --path timestamp-lsp` \
             from the zed-timestamp repo, then restart Zed."
                .to_string()
        })?;
        Ok(zed::Command {
            command: path,
            args: vec![],
            env: Default::default(),
        })
    }
}

zed::register_extension!(TimestampExtension);
