use std::path::{Path, PathBuf};

use anyhow::Result;

/// A resolved editor — binary path and any extra arguments needed.
pub struct Editor {
    pub bin: PathBuf,
    pub args: Vec<String>,
}

/// Resolve the editor using the priority chain:
/// `$PODBOX_EDITOR` > `$VISUAL` > `$EDITOR` > nvim > neovim > hx > helix > code > nano > vi.
pub fn resolve() -> Result<Editor> {
    let env_editor = std::env::var("PODBOX_EDITOR").ok();
    let env_visual = std::env::var("VISUAL").ok();
    let env_editor_generic = std::env::var("EDITOR").ok();

    let candidates: Vec<(&str, Option<&str>)> = vec![
        ("$PODBOX_EDITOR", env_editor.as_deref()),
        ("$VISUAL", env_visual.as_deref()),
        ("$EDITOR", env_editor_generic.as_deref()),
        ("nvim", None),
        ("neovim", None),
        ("hx", None),
        ("helix", None),
        ("code", None),
        ("nano", None),
        ("vi", None),
    ];

    for (name, env_val) in &candidates {
        let path = match env_val {
            Some(val) => {
                let parts = shell_words::split(val).unwrap_or_else(|_| vec![val.to_string()]);
                let bin_part = parts.first().map(|s| s.as_str()).unwrap_or(val);
                let p = PathBuf::from(bin_part);
                if p.is_absolute() && p.exists() {
                    p
                } else {
                    match which::which(bin_part) {
                        Ok(p) => p,
                        Err(_) => continue,
                    }
                }
            }
            None => match which::which(name) {
                Ok(p) => p,
                Err(_) => continue,
            },
        };

        let mut args = editor_args(name);
        if let Some(val) = env_val {
            if let Ok(words) = shell_words::split(val) {
                args.extend(words.into_iter().skip(1));
            }
        }
        let editor = Editor { bin: path, args };
        return Ok(editor);
    }

    anyhow::bail!(
        "no editor found.\n\
         Set $VISUAL, $EDITOR, or $PODBOX_EDITOR to your preferred editor.\n\
         Example: export EDITOR=nano"
    );
}

/// Return extra args needed for the given editor binary name.
/// VS Code (`code` / `code-insiders`) needs `--wait`.
fn editor_args(name: &str) -> Vec<String> {
    let base = Path::new(name)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| name.to_string());
    if base == "code" || base == "code-insiders" {
        vec!["--wait".into()]
    } else {
        vec![]
    }
}

/// Open the given file in the editor. Blocks until the editor exits.
pub fn open(editor: &Editor, path: &Path) -> Result<()> {
    let status = std::process::Command::new(&editor.bin)
        .args(&editor.args)
        .arg(path)
        .status()?;

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        anyhow::bail!("editor exited with code {}", code);
    }
    Ok(())
}
