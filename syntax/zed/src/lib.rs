//! Starts `song-lsp` for song files (#482): Zed needs an extension to run a
//! language server of its own. The server is found on the worktree's PATH.

use zed_extension_api::{self as zed, LanguageServerId, Result};

struct Song;

impl zed::Extension for Song {
    fn new() -> Self {
        Song
    }

    fn language_server_command(
        &mut self,
        _id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let command = worktree
            .which("song-lsp")
            .ok_or("song-lsp is not on PATH: cargo install --path syntax/song-lsp".to_string())?;
        Ok(zed::Command {
            command,
            args: Vec::new(),
            env: worktree.shell_env(),
        })
    }
}

zed::register_extension!(Song);
