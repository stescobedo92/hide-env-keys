//! Integration tests against a corpus of source files in each
//! supported language.

#![allow(
    clippy::expect_used,
    clippy::panic,
    // Shell `${VAR}` fixtures look like format strings to clippy, but they
    // are deliberate shell-syntax test data, not Rust format specifiers.
    clippy::literal_string_with_formatting_args
)]

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use evault_core::error::ScannerError;
use evault_core::traits::CodeScanner;
use evault_scanner_regex::RegexCodeScanner;
use tempfile::TempDir;

fn write(dir: &Path, file: &str, body: &str) {
    let path = dir.join(file);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create dir");
    }
    fs::write(path, body).expect("write");
}

fn names(hits: &[evault_core::traits::ScanHit]) -> HashSet<&str> {
    hits.iter().map(|h| h.name.as_str()).collect()
}

#[test]
fn javascript_typescript_corpus() {
    let dir = TempDir::new().expect("tmpdir");
    write(
        dir.path(),
        "app.js",
        "const a = process.env.DATABASE_URL;\n\
         const b = process.env[\"API_KEY\"];\n\
         const c = process.env['NODE_ENV'];\n\
         const d = process.env.production; // lowercase: NOT an env var\n",
    );
    write(
        dir.path(),
        "server.ts",
        "const port = process.env.PORT ?? 3000;\n",
    );

    let hits = RegexCodeScanner::new().scan(dir.path()).expect("scan");
    let found = names(&hits);
    assert_eq!(
        hits.len(),
        4,
        "expected exactly 4 hits (DATABASE_URL, API_KEY, NODE_ENV, PORT)"
    );
    assert!(found.contains("DATABASE_URL"));
    assert!(found.contains("API_KEY"));
    assert!(found.contains("NODE_ENV"));
    assert!(found.contains("PORT"));
    assert!(!found.contains("production"), "lowercase must not match");
}

#[test]
fn python_corpus() {
    let dir = TempDir::new().expect("tmpdir");
    write(
        dir.path(),
        "app.py",
        "import os\n\
         a = os.getenv(\"DATABASE_URL\")\n\
         b = os.getenv('API_KEY', 'default')\n\
         c = os.environ[\"NODE_ENV\"]\n\
         d = os.environ.get('LOG_LEVEL')\n",
    );

    let hits = RegexCodeScanner::new().scan(dir.path()).expect("scan");
    let found = names(&hits);
    assert_eq!(hits.len(), 4);
    assert!(found.contains("DATABASE_URL"));
    assert!(found.contains("API_KEY"));
    assert!(found.contains("NODE_ENV"));
    assert!(found.contains("LOG_LEVEL"));
}

#[test]
fn rust_corpus() {
    let dir = TempDir::new().expect("tmpdir");
    write(
        dir.path(),
        "src/main.rs",
        "fn main() {\n\
             let db = std::env::var(\"DATABASE_URL\").unwrap();\n\
             let key = env::var(\"API_KEY\").ok();\n\
             let path = std::env::var_os(\"PATH\");\n\
         }\n",
    );

    let hits = RegexCodeScanner::new().scan(dir.path()).expect("scan");
    let found = names(&hits);
    assert_eq!(hits.len(), 3);
    assert!(found.contains("DATABASE_URL"));
    assert!(found.contains("API_KEY"));
    assert!(found.contains("PATH"));
}

#[test]
fn go_corpus() {
    let dir = TempDir::new().expect("tmpdir");
    write(
        dir.path(),
        "main.go",
        "package main\n\
         import \"os\"\n\
         func main() {\n\
             db := os.Getenv(\"DATABASE_URL\")\n\
             _, ok := os.LookupEnv(\"API_KEY\")\n\
             _ = db\n\
             _ = ok\n\
         }\n",
    );

    let hits = RegexCodeScanner::new().scan(dir.path()).expect("scan");
    let found = names(&hits);
    assert_eq!(hits.len(), 2);
    assert!(found.contains("DATABASE_URL"));
    assert!(found.contains("API_KEY"));
}

#[test]
fn shell_corpus() {
    let dir = TempDir::new().expect("tmpdir");
    write(
        dir.path(),
        "deploy.sh",
        "#!/usr/bin/env bash\n\
         echo \"$DATABASE_URL\"\n\
         echo \"${API_KEY}\"\n\
         echo \"${NODE_ENV:-development}\"\n\
         echo \"${HOME#/}\"\n\
         echo \"${PATH%%:*}\"\n\
         # lowercase $foo must NOT match.\n\
         echo \"$foo\"\n",
    );

    let hits = RegexCodeScanner::new().scan(dir.path()).expect("scan");
    let found = names(&hits);
    assert!(found.contains("DATABASE_URL"));
    assert!(found.contains("API_KEY"));
    assert!(found.contains("NODE_ENV"));
    assert!(found.contains("HOME"));
    assert!(found.contains("PATH"));
    assert!(!found.contains("foo"));
}

#[test]
fn skipped_directories_are_not_descended() {
    let dir = TempDir::new().expect("tmpdir");
    // A real source file in src/ should be picked up.
    write(
        dir.path(),
        "src/app.js",
        "const a = process.env.REAL_VAR;\n",
    );
    // node_modules contains a fake reference that must be IGNORED.
    write(
        dir.path(),
        "node_modules/some-pkg/index.js",
        "const a = process.env.IGNORED_VAR;\n",
    );
    write(
        dir.path(),
        "target/debug/build.rs",
        "let a = std::env::var(\"ALSO_IGNORED\");\n",
    );
    write(dir.path(), ".git/config.js", "process.env.GIT_IGNORED;\n");
    // Recently added skipped dirs — regression coverage for SKIPPED_DIRS
    // expansions (`.terraform`, `.next`, `coverage`, etc.).
    write(
        dir.path(),
        ".terraform/providers/foo.tf.js",
        "process.env.TERRAFORM_IGNORED;\n",
    );
    write(
        dir.path(),
        ".next/server/pages.js",
        "process.env.NEXT_IGNORED;\n",
    );
    write(
        dir.path(),
        "coverage/lcov-report/script.js",
        "process.env.COVERAGE_IGNORED;\n",
    );

    let hits = RegexCodeScanner::new().scan(dir.path()).expect("scan");
    let found = names(&hits);
    assert_eq!(hits.len(), 1, "only REAL_VAR should be picked up");
    assert!(found.contains("REAL_VAR"));
    assert!(!found.contains("IGNORED_VAR"));
    assert!(!found.contains("ALSO_IGNORED"));
    assert!(!found.contains("GIT_IGNORED"));
    assert!(!found.contains("TERRAFORM_IGNORED"));
    assert!(!found.contains("NEXT_IGNORED"));
    assert!(!found.contains("COVERAGE_IGNORED"));
}

#[test]
fn unknown_extensions_are_skipped() {
    let dir = TempDir::new().expect("tmpdir");
    write(
        dir.path(),
        "README.md",
        "Use `process.env.FOO` in your code.\n",
    );
    write(dir.path(), "config.toml", "foo = \"$BAR\"\n");

    let hits = RegexCodeScanner::new().scan(dir.path()).expect("scan");
    assert!(hits.is_empty(), "unknown extensions must not be scanned");
}

#[test]
fn line_and_column_are_one_based() {
    let dir = TempDir::new().expect("tmpdir");
    // The interesting reference is on line 3, after 4 columns of
    // leading whitespace + `const a = `.
    write(
        dir.path(),
        "app.js",
        "// comment line 1\n\
         // comment line 2\n\
         const a = process.env.FOO;\n",
    );
    let hits = RegexCodeScanner::new().scan(dir.path()).expect("scan");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].line, 3);
    // The `FOO` capture starts at column 23 (1-based) of the line
    // `const a = process.env.FOO;`. Counting:
    // `const a = process.env.` is 22 chars; FOO begins at col 23.
    assert_eq!(hits[0].column, 23);
}

/// Regression test for the contract on [`ScanHit::column`]: 1-based
/// **character** offset, not byte offset. A line containing multi-byte
/// UTF-8 before the match would diverge if we reported bytes.
#[test]
fn column_is_a_character_offset_not_a_byte_offset() {
    let dir = TempDir::new().expect("tmpdir");
    // `日本語` is three CJK characters, 9 bytes in UTF-8. The match
    // `FOO` begins at character offset 25 (1-based): 8 chars of
    // `// 日本語 ` followed by `const a = process.env.` (22 chars).
    //
    // Wait — let's just count precisely. Comment prefix is `// `
    // (3 chars) + `日本語` (3 chars) + ` ` (1 char) = 7 chars, then
    // `process.env.` is 12 chars, so FOO starts at char 7+12+1 = 20.
    // We assert by computation rather than guessing.
    let line = "// 日本語 process.env.FOO";
    let expected_col = line.find("FOO").map(|byte_idx| {
        line.get(..byte_idx)
            .map_or(byte_idx, |prefix| prefix.chars().count())
            + 1
    });

    write(dir.path(), "app.js", &format!("{line}\n"));
    let hits = RegexCodeScanner::new().scan(dir.path()).expect("scan");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].name, "FOO");
    assert_eq!(Some(hits[0].column), expected_col);
    // And confirm it is NOT the byte offset (those differ since the
    // CJK chars consume 9 bytes but only 3 characters).
    let byte_col = line.find("FOO").map(|b| b + 1).unwrap();
    assert_ne!(
        hits[0].column, byte_col,
        "column should be characters, not bytes"
    );
}

#[test]
fn binary_files_are_silently_skipped() {
    let dir = TempDir::new().expect("tmpdir");
    // Write invalid UTF-8 to a `.js` file — `read_to_string` will
    // return InvalidData and the scanner must swallow it.
    fs::write(dir.path().join("bin.js"), b"\xff\xfe\xfd\xfc invalid utf-8").expect("write");
    write(dir.path(), "ok.js", "const a = process.env.STILL_FOUND;\n");

    let hits = RegexCodeScanner::new().scan(dir.path()).expect("scan");
    let found: HashSet<&str> = names(&hits);
    assert!(found.contains("STILL_FOUND"));
    // No panic, no error — the binary file is silently skipped.
}

#[test]
fn multiple_references_on_one_line_each_produce_a_hit() {
    let dir = TempDir::new().expect("tmpdir");
    write(
        dir.path(),
        "app.js",
        "const url = process.env.A + ':' + process.env.B;\n",
    );
    let hits = RegexCodeScanner::new().scan(dir.path()).expect("scan");
    assert_eq!(hits.len(), 2);
    let found = names(&hits);
    assert!(found.contains("A"));
    assert!(found.contains("B"));
}

/// Trying to walk a nonexistent root must surface as `ScannerError::Io`,
/// not silently return an empty `Vec`. The error message should include
/// the offending path so the caller can diagnose it.
#[test]
fn nonexistent_root_returns_io_error() {
    let dir = TempDir::new().expect("tmpdir");
    let bogus = dir.path().join("does-not-exist");

    match RegexCodeScanner::new().scan(&bogus) {
        Err(ScannerError::Io(e)) => {
            // The error message should mention the offending path so a
            // human reading it can act on it.
            let msg = format!("{e}");
            assert!(
                msg.contains("does-not-exist"),
                "error must mention the offending path; got: {msg}"
            );
        }
        Err(other) => panic!("expected Io error, got: {other:?}"),
        Ok(hits) => panic!("expected error, got {} hits", hits.len()),
    }
}

#[test]
fn default_and_new_produce_equivalent_scanners() {
    let dir = TempDir::new().expect("tmpdir");
    write(dir.path(), "app.js", "process.env.SAME;\n");
    let via_new = RegexCodeScanner::new().scan(dir.path()).expect("scan");
    let via_default = RegexCodeScanner::default().scan(dir.path()).expect("scan");
    assert_eq!(via_new.len(), via_default.len());
    assert_eq!(via_new[0].name, via_default[0].name);
}
