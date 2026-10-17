//! Explicit untracked dependency/cache output; Git ignore rules grant no authority.
use std::path::{Component, Path};

/// These directory names have package-tool output semantics at every workspace
/// depth. Only descendants qualify: a file or symlink named `node_modules`
/// itself is still an input. Broad source roots (`apps`, `packages`, `vendor`)
/// and ambiguous names (`build`, `dist`, `venv`) are intentionally excluded.
/// The caller preserves declared-input precedence and checks tracked changes.
pub(super) fn is_generated_dependency(path: &str) -> bool {
    let mut components = Path::new(path).components().peekable();
    while let Some(component) = components.next() {
        if components.peek().is_some()
            && matches!(component, Component::Normal(name) if matches!(
                name.to_str(), Some("node_modules" | ".venv" | ".ruff_cache" | ".pytest_cache" | "__pycache__")
            ))
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::is_generated_dependency;

    #[test]
    fn exact_dependency_directory_components_are_generated() {
        for path in [
            "node_modules/.bun/pkg@1/node_modules/pkg/index.js",
            "apps/web/node_modules/pkg/index.js",
            "packages/ui/node_modules/.bin/compiler",
            "vendor/tool/.venv/lib/python3/site-packages/tool.py",
            ".venv/pyvenv.cfg",
            "apps/api/.ruff_cache/.gitignore",
            ".ruff_cache/0.14/cache",
            "packages/lib/__pycache__/lib.cpython-314.pyc",
            ".pytest_cache/v/cache/nodeids",
            "deps/aurora-ui/node_modules/pkg/index.js",
            "deps/product-media-kit/tool/__pycache__/tool.cpython-314.pyc",
        ] {
            assert!(is_generated_dependency(path), "{path}");
        }
    }

    #[test]
    fn source_roots_and_similar_names_are_inputs() {
        for path in [
            "apps/web/src/index.ts",
            "packages/ui/package.json",
            "vendor/tool/input.py",
            "deps/product-media-kit/tool/input.py",
            "src/node_modules_backup/index.ts",
            "node_modules.rs",
            ".venv-source/main.py",
            "apps/api/.ruff_cache_config.json",
            "apps/api/.pytest_cache_config.json",
            "src/__pycache__helpers/lib.py",
            "node_modules",
            ".venv",
            "dist/index.js",
            "venv/source.py",
        ] {
            assert!(!is_generated_dependency(path), "{path}");
        }
    }
}
