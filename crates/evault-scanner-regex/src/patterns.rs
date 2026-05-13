//! Per-language regex pattern banks.
//!
//! Each [`LanguageSpec`] couples a set of file extensions with a list of
//! compiled [`regex::Regex`] patterns. Every pattern must define a single
//! named capture group `name` containing the variable identifier.
//!
//! The patterns are deliberately strict: they match only the
//! conventional `[A-Z_][A-Z0-9_]*` env-var shape. This matches
//! [`evault_core::model::Var::validate_name`]'s rule and keeps the
//! false-positive rate low — a JS expression like
//! `obj.processError(env)` will not be mistaken for an env reference.

use regex::Regex;

/// One supported language: which file extensions opt in and which
/// patterns apply.
pub struct LanguageSpec {
    /// File extensions (without leading dot) that the scanner treats as
    /// this language's source code.
    pub extensions: &'static [&'static str],
    /// Compiled patterns. Each must have exactly one named group `name`.
    pub patterns: Vec<Regex>,
}

/// Identifier shape used inside every pattern.
///
/// `[A-Z_][A-Z0-9_]*` matches the conventional env-var name (matches
/// `Var::validate_name`'s "must start with letter/underscore, then
/// alphanumerics/underscore"). The pattern excludes lowercase to keep
/// false positives down — a JS expression like `process.env.production`
/// (looking up a config key, not a real env var) won't match.
const IDENT: &str = r"[A-Z_][A-Z0-9_]*";

/// Build the full pattern bank.
///
/// # Panics
/// Panics if a pattern literal fails to compile. The patterns are all
/// fixed at build time and exercised by unit tests, so a panic here
/// means the source was edited incorrectly.
#[allow(clippy::expect_used)]
pub fn language_specs() -> Vec<LanguageSpec> {
    let compile = |s: &str| Regex::new(s).expect("static pattern must compile");

    vec![
        // JavaScript / TypeScript.
        LanguageSpec {
            extensions: &["js", "mjs", "cjs", "ts", "tsx", "jsx"],
            patterns: vec![
                // process.env.NAME
                compile(&format!(r"process\.env\.(?P<name>{IDENT})\b")),
                // process.env["NAME"] / process.env['NAME']
                compile(&format!(
                    r#"process\.env\[\s*["'](?P<name>{IDENT})["']\s*\]"#
                )),
            ],
        },
        // Python.
        LanguageSpec {
            extensions: &["py"],
            patterns: vec![
                // os.getenv("NAME"[, default])
                compile(&format!(r#"os\.getenv\(\s*["'](?P<name>{IDENT})["']"#)),
                // os.environ["NAME"] / os.environ.get("NAME")
                compile(&format!(
                    r#"os\.environ\[\s*["'](?P<name>{IDENT})["']\s*\]"#
                )),
                compile(&format!(
                    r#"os\.environ\.get\(\s*["'](?P<name>{IDENT})["']"#
                )),
            ],
        },
        // Rust.
        LanguageSpec {
            extensions: &["rs"],
            patterns: vec![
                // std::env::var("NAME") / env::var("NAME")
                compile(&format!(
                    r#"(?:std::)?env::var(?:_os)?\(\s*"(?P<name>{IDENT})""#
                )),
            ],
        },
        // Go.
        LanguageSpec {
            extensions: &["go"],
            patterns: vec![
                compile(&format!(r#"os\.Getenv\(\s*"(?P<name>{IDENT})""#)),
                compile(&format!(r#"os\.LookupEnv\(\s*"(?P<name>{IDENT})""#)),
            ],
        },
        // Shell.
        LanguageSpec {
            extensions: &["sh", "bash"],
            patterns: vec![
                // ${NAME} and its parameter-expansion variants. We
                // anchor with `${` explicitly so `$()` command
                // substitution doesn't match. The terminator class
                // covers every standard parameter-expansion form
                // where NAME comes FIRST inside the braces:
                //   `}`  bare        — `${VAR}`
                //   `:`  defaults    — `${VAR:-x}`, `${VAR:+x}`, `${VAR:?x}`, `${VAR:o:l}`
                //   `-`  use-default — `${VAR-x}`   (POSIX, no leading colon)
                //   `=`  assign-def  — `${VAR=x}`   (POSIX, no leading colon)
                //   `+`  use-alt     — `${VAR+x}`   (POSIX, no leading colon)
                //   `?`  error-set   — `${VAR?msg}` (POSIX, no leading colon)
                //   `#`  prefix      — `${VAR#x}`, `${VAR##x}`
                //   `%`  suffix      — `${VAR%x}`, `${VAR%%x}`
                //   `^`  upper case  — `${VAR^}`, `${VAR^^}`
                //   `,`  lower case  — `${VAR,}`, `${VAR,,}`
                //   `/`  substitute  — `${VAR/from/to}`
                //   `[`  array idx   — `${VAR[0]}`
                compile(&format!(r"\$\{{(?P<name>{IDENT})[-:}}#%^,/+=?\[]")),
                // Sigil-prefix forms where the operator comes BEFORE
                // the identifier: `${!VAR}` (indirect expansion) and
                // `${#VAR}` (length). The closing brace anchors the
                // match so that `${#}` (length of `$@`) does not
                // produce a stray empty hit.
                compile(&format!(r"\$\{{[!#](?P<name>{IDENT})\}}")),
                // Bare $NAME at a word boundary. We require uppercase
                // identifier to keep `$foo` from matching.
                compile(&format!(r"\$(?P<name>{IDENT})\b")),
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture_name(pattern: &Regex, haystack: &str) -> Option<String> {
        pattern
            .captures(haystack)
            .and_then(|c| c.name("name").map(|m| m.as_str().to_owned()))
    }

    #[test]
    fn js_process_env_dot() {
        let specs = language_specs();
        let js = &specs
            .iter()
            .find(|s| s.extensions.contains(&"js"))
            .unwrap()
            .patterns;
        assert_eq!(
            capture_name(&js[0], "process.env.DATABASE_URL"),
            Some("DATABASE_URL".into())
        );
        assert_eq!(
            capture_name(&js[0], "process.env.NODE_ENV;"),
            Some("NODE_ENV".into())
        );
        // Lowercase rejected.
        assert!(capture_name(&js[0], "process.env.production").is_none());
    }

    #[test]
    fn js_process_env_bracket() {
        let specs = language_specs();
        let js = &specs
            .iter()
            .find(|s| s.extensions.contains(&"js"))
            .unwrap()
            .patterns;
        assert_eq!(
            capture_name(&js[1], r#"process.env["DATABASE_URL"]"#),
            Some("DATABASE_URL".into())
        );
        assert_eq!(
            capture_name(&js[1], r"process.env['API_KEY']"),
            Some("API_KEY".into())
        );
    }

    #[test]
    fn python_getenv_and_environ() {
        let specs = language_specs();
        let py = &specs
            .iter()
            .find(|s| s.extensions.contains(&"py"))
            .unwrap()
            .patterns;
        assert_eq!(
            capture_name(&py[0], r#"os.getenv("DATABASE_URL")"#),
            Some("DATABASE_URL".into())
        );
        assert_eq!(
            capture_name(&py[0], r#"os.getenv("FOO", "default")"#),
            Some("FOO".into())
        );
        assert_eq!(
            capture_name(&py[1], r#"os.environ["NODE_ENV"]"#),
            Some("NODE_ENV".into())
        );
        assert_eq!(
            capture_name(&py[2], r#"os.environ.get("FOO")"#),
            Some("FOO".into())
        );
    }

    #[test]
    fn rust_env_var() {
        let specs = language_specs();
        let rs = &specs
            .iter()
            .find(|s| s.extensions.contains(&"rs"))
            .unwrap()
            .patterns;
        assert_eq!(
            capture_name(&rs[0], r#"std::env::var("DATABASE_URL")"#),
            Some("DATABASE_URL".into())
        );
        assert_eq!(
            capture_name(&rs[0], r#"env::var("FOO")"#),
            Some("FOO".into())
        );
        assert_eq!(
            capture_name(&rs[0], r#"std::env::var_os("BAR")"#),
            Some("BAR".into())
        );
    }

    #[test]
    fn go_getenv() {
        let specs = language_specs();
        let go = &specs
            .iter()
            .find(|s| s.extensions.contains(&"go"))
            .unwrap()
            .patterns;
        assert_eq!(
            capture_name(&go[0], r#"os.Getenv("DATABASE_URL")"#),
            Some("DATABASE_URL".into())
        );
        assert_eq!(
            capture_name(&go[1], r#"os.LookupEnv("FOO")"#),
            Some("FOO".into())
        );
    }

    #[test]
    fn shell_braces_and_bare() {
        let specs = language_specs();
        let sh = &specs
            .iter()
            .find(|s| s.extensions.contains(&"sh"))
            .unwrap()
            .patterns;
        assert_eq!(
            capture_name(&sh[0], "echo ${DATABASE_URL}"),
            Some("DATABASE_URL".into())
        );
        // `${FOO:-default}` is shell parameter-default syntax, not a Rust
        // format-string; the clippy lint mis-identifies it.
        #[allow(clippy::literal_string_with_formatting_args)]
        let braced_default = "echo ${FOO:-default}";
        assert_eq!(capture_name(&sh[0], braced_default), Some("FOO".into()));
        // sh[2] is the bare `$NAME` pattern (sh[1] is the sigil-prefix
        // `${!VAR}` / `${#VAR}` form).
        assert_eq!(
            capture_name(&sh[2], "echo $DATABASE_URL"),
            Some("DATABASE_URL".into())
        );
        // Lowercase rejected.
        assert!(capture_name(&sh[2], "echo $foo").is_none());
    }

    /// Every standard parameter-expansion terminator should be
    /// matched. These previously fell through `${VAR}` only, causing
    /// silent undercount on shell scripts using POSIX-portable forms
    /// (`${VAR-x}`, `${VAR+x}`, …) or bash prefix/suffix/case operators.
    #[test]
    fn shell_brace_recognises_all_parameter_expansion_forms() {
        let specs = language_specs();
        let sh = &specs
            .iter()
            .find(|s| s.extensions.contains(&"sh"))
            .unwrap()
            .patterns;
        let cases = [
            // bash prefix/suffix/case/substitute/array forms
            ("echo ${PATH#prefix}", "PATH"),
            ("echo ${PATH##prefix}", "PATH"),
            ("echo ${VAR%suffix}", "VAR"),
            ("echo ${VAR%%suffix}", "VAR"),
            ("echo ${VAR^^}", "VAR"),
            ("echo ${VAR,,}", "VAR"),
            ("echo ${VAR/from/to}", "VAR"),
            ("echo ${VAR[0]}", "VAR"),
            // POSIX-portable forms without the leading colon
            ("echo ${VAR-default}", "VAR"),
            ("echo ${VAR=default}", "VAR"),
            ("echo ${VAR+alt}", "VAR"),
            ("echo ${VAR?msg}", "VAR"),
        ];
        for (haystack, expected) in cases {
            assert_eq!(
                capture_name(&sh[0], haystack).as_deref(),
                Some(expected),
                "missed: {haystack}"
            );
        }
    }

    /// Sigil-prefix forms `${!VAR}` (indirect) and `${#VAR}` (length)
    /// are matched by the second shell pattern. The previous brace
    /// regex required the identifier to come first inside `${...}` and
    /// silently dropped these.
    #[test]
    fn shell_sigil_prefix_forms_indirect_and_length() {
        let specs = language_specs();
        let sh = &specs
            .iter()
            .find(|s| s.extensions.contains(&"sh"))
            .unwrap()
            .patterns;
        assert_eq!(capture_name(&sh[1], "echo ${!VAR}").as_deref(), Some("VAR"));
        assert_eq!(
            capture_name(&sh[1], "echo ${#PATH}").as_deref(),
            Some("PATH")
        );
        // The trailing `}` is required: `${#}` (length of positional
        // params) must not produce an empty / stray match.
        assert!(capture_name(&sh[1], "echo ${#}").is_none());
    }
}
