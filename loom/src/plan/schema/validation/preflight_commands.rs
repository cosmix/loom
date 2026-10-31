//! Build-tool diagnostics for individual shell invocations.

use std::path::{Path, PathBuf};

use super::shell_lex::Word;
use super::v2_lints::visit_argvs;

const TOOLS: &[(&str, &str)] = &[
    ("cargo", "Cargo.toml"),
    ("npm", "package.json"),
    ("bun", "package.json"),
    ("yarn", "package.json"),
    ("go", "go.mod"),
    ("pytest", "pyproject.toml"),
    ("uv", "pyproject.toml"),
];

fn selector<'a>(argv: &'a [&Word], flag: &str) -> Option<&'a str> {
    for (idx, word) in argv.iter().enumerate() {
        if word.value == "--" {
            break;
        }
        if word.value == flag {
            return argv
                .get(idx + 1)
                .filter(|value| !value.expands)
                .map(|value| value.value.as_str());
        }
        if !word.expands {
            if let Some(value) = word
                .value
                .strip_prefix(flag)
                .and_then(|rest| rest.strip_prefix('='))
            {
                return Some(value);
            }
        }
    }
    None
}

fn config_path(argv: &[&Word], tool: &str, config: &str, cwd: &Path) -> PathBuf {
    let selected = match tool {
        "cargo" => selector(argv, "--manifest-path").map(|value| cwd.join(value)),
        "bun" => selector(argv, "--project-dir").map(|value| cwd.join(value).join(config)),
        "go" => selector(argv, "-C").map(|value| cwd.join(value).join(config)),
        _ => None,
    };
    selected.unwrap_or_else(|| cwd.join(config))
}

pub(super) fn warn(stage: &str, command: &str, cwd: &Path, warnings: &mut Vec<String>) {
    visit_argvs(command, 0, &mut |argv| {
        let executable = argv[0];
        if argv.iter().any(|word| word.expands) {
            return;
        }
        let Some(&(tool, config)) = TOOLS
            .iter()
            .find(|(tool, _)| *tool == executable.command_name())
        else {
            return;
        };
        let path = config_path(argv, tool, config, cwd);
        if !path.exists() {
            warnings.push(format!("Stage '{stage}': Acceptance criterion '{command}' uses '{tool}' but {config} not found at {}", path.display()));
        }
    });
}

pub(super) fn has_working_dir_prefix(path: &str, working_dir: &str) -> bool {
    Path::new(path).starts_with(Path::new(working_dir))
}
