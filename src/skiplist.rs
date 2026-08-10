//! Directory/file skip list, kept identical to gm's own
//! (rs-plugkit crates/plugkit-core/src/code_index.rs SKIP_DIRS /
//! SKIP_FILE_SUFFIXES) so a directory scanned by crux and a directory
//! scanned by gm's own code-index agree on what counts as noise --
//! build artifacts, vendored dependencies, caches, binary/media files.

pub const SKIP_DIRS: &[&str] = &[
    ".git", ".svn", ".hg", ".bzr", "CVS", ".gm",
    "node_modules", ".npm", ".yarn", ".pnp", ".next", ".nuxt", "dist", "out",
    "build", ".cache", ".parcel-cache", ".vite", ".turbo", ".nx", ".rush",
    ".lerna", ".pnpm-store", ".docusaurus", ".vuepress",
    "__pycache__", ".pytest_cache", ".mypy_cache", ".hypothesis", ".pyre",
    ".pytype", "env", "venv", "ENV", ".venv", ".tox", "htmlcov", "site-packages",
    "target",
    "vendor",
    ".gradle", ".mvn", "bin", "obj",
    ".bundle",
    "Pods", "DerivedData",
    ".terraform", ".serverless",
    ".docker",
    ".llamaindex", ".chroma", ".vectorstore", ".embeddings", ".langchain",
    "embeddings", "vector-db", "faiss-index", "chromadb",
    ".claude", ".wfgy", ".kilo", ".agents", ".code-search",
    ".plugkit-browser-profile-default", ".plugkit-agent-worktree",
    ".test-chrome-profile",
    ".vscode", ".idea", ".vs", ".sublime-text", ".cursor", ".windsurf",
    ".zed", ".helix",
    "coverage", ".nyc_output", "test-results", "playwright-report",
    ".plugkit-browser-profile",
    "_site", "public", "static", "site", "output", "builds", "artifacts",
    "compiled", "generated", "gen",
    "Carthage", "fastlane",
    "mlruns", "wandb", "weights",
    ".cargo", ".rustup", ".rbenv", ".rvm", ".nvm", ".pyenv", ".conda",
    ".m2", ".sbt", ".ivy2", ".gem",
];

pub const SKIP_FILE_SUFFIXES: &[&str] = &[
    ".min.js", ".min.css", ".bundle.js", ".chunk.js", ".map",
    "package-lock.json", "yarn.lock", "pnpm-lock.yaml", "bun.lockb",
    "bun.lock", "Cargo.lock", "composer.lock", "Gemfile.lock", "poetry.lock",
    "Pipfile.lock", "go.sum", "uv.lock",
    ".codeinsight", ".codeinsight.digest", ".perf-baseline.json",
    ".rs-exec.lock",
    ".glb", ".gltf", ".vrm", ".fbx", ".blend", ".blend1", ".usdz", ".hf",
    ".uasset", ".umap",
    ".wasm", ".exe", ".dll", ".dylib", ".so", ".o", ".obj", ".a", ".lib",
    ".pdb", ".class", ".jar", ".war", ".ear", ".apk", ".aab", ".ipa",
    ".hex", ".elf", ".uf2", ".dfu",
    ".png", ".jpg", ".jpeg", ".gif", ".ico", ".bmp", ".webp", ".tiff",
    ".pdf", ".mov", ".mp4", ".avi", ".flv", ".mkv", ".webm", ".mp3",
    ".m4a", ".wav", ".flac", ".ogg", ".woff", ".woff2", ".ttf", ".otf",
    ".eot", ".zip", ".tar", ".tar.gz", ".tgz", ".rar", ".7z", ".iso",
    ".bz2", ".xz", ".lz4", ".zst", ".cab", ".deb", ".rpm", ".dmg", ".msi",
    ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx",
    ".psd", ".ai", ".sketch", ".aep",
    ".pkl", ".pickle", ".h5", ".hdf5", ".parquet", ".npy", ".npz",
    ".safetensors", ".ckpt", ".pt", ".pth", ".onnx", ".gguf",
    "tokenizer.json", "vocab.json", "vocab.txt", "merges.txt",
    "-tokenizer.json", "-vocab.json",
    ".stackdump", ".dmp", ".core",
    ".key", ".pem", ".p12", ".pfx", ".p8", ".crt", ".cer", ".der",
    "credentials.json", "secrets.yaml", "secrets.yml",
    ".db", ".sqlite", ".sqlite3",
];

pub fn is_skipped_dir_segment(seg: &str) -> bool {
    SKIP_DIRS.contains(&seg)
}

pub fn is_skipped_filename(name: &str) -> bool {
    SKIP_FILE_SUFFIXES.iter().any(|suf| name.ends_with(suf))
}

pub fn is_hidden_segment(seg: &str) -> bool {
    seg.starts_with('.') && seg != "." && seg != ".."
}

/// Minimal noise list for `--mode files`/`--mode gitlog`, deliberately
/// smaller than SKIP_DIRS above. That list is tuned for a code-search
/// index (skip anything that isn't source worth grepping), which
/// necessarily also excludes generically-named directories that are
/// nonetheless real, authored project content on some repos --
/// `site`/`weights`/`.cargo` on this very project. Structural-outlier
/// scanning's whole premise is "let rarity decide, don't hardcode an
/// opinion about what's noise", so this list only excludes what is
/// unconditionally never authored content: VCS internals and the handful
/// of build-output directory names confirmed by ecosystem convention to
/// be pure derived output, never a place a human puts real files.
pub const STRUCTURAL_SKIP_DIRS: &[&str] = &[
    ".git", ".svn", ".hg", ".bzr", "CVS",
    "node_modules", "target", "dist", "build", "__pycache__",
    ".venv", "venv", "ENV",
];

pub fn is_structural_skip_dir(seg: &str) -> bool {
    STRUCTURAL_SKIP_DIRS.contains(&seg)
}
