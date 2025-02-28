use std::path::Path;

use crate::error::Result;
use ignored_suffixes::IgnoredSuffixes;

use globset::{Candidate, GlobBuilder, GlobMatcher};

pub mod ignored_suffixes;

#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum MappingTarget<'a> {
    /// For mapping a path to a specific syntax.
    MapTo(&'a str),

    /// For mapping a path (typically an extension-less file name) to an unknown
    /// syntax. This typically means later using the contents of the first line
    /// of the file to determine what syntax to use.
    MapToUnknown,

    /// For mapping a file extension (e.g. `*.conf`) to an unknown syntax. This
    /// typically means later using the contents of the first line of the file
    /// to determine what syntax to use. However, if a syntax handles a file
    /// name that happens to have the given file extension (e.g. `resolv.conf`),
    /// then that association will have higher precedence, and the mapping will
    /// be ignored.
    MapExtensionToUnknown,
}

#[derive(Debug, Clone, Default)]
pub struct SyntaxMapping<'a> {
    mappings: Vec<(GlobMatcher, MappingTarget<'a>)>,
    pub(crate) ignored_suffixes: IgnoredSuffixes<'a>,
}

impl<'a> SyntaxMapping<'a> {
    pub fn empty() -> SyntaxMapping<'a> {
        Default::default()
    }

    pub fn builtin() -> SyntaxMapping<'a> {
        let mut mapping = Self::empty();
        mapping.insert("*.h", MappingTarget::MapTo("C++")).unwrap();
        mapping.insert("*.fs", MappingTarget::MapTo("F#")).unwrap();
        mapping
            .insert("build", MappingTarget::MapToUnknown)
            .unwrap();
        mapping
            .insert("**/.ssh/config", MappingTarget::MapTo("SSH Config"))
            .unwrap();
        mapping
            .insert(
                "**/bat/config",
                MappingTarget::MapTo("Bourne Again Shell (bash)"),
            )
            .unwrap();
        mapping
            .insert(
                "/etc/profile",
                MappingTarget::MapTo("Bourne Again Shell (bash)"),
            )
            .unwrap();
        mapping
            .insert("*.pac", MappingTarget::MapTo("JavaScript (Babel)"))
            .unwrap();

        // See #1008
        mapping
            .insert("rails", MappingTarget::MapToUnknown)
            .unwrap();

        // Nginx and Apache syntax files both want to style all ".conf" files
        // see #1131 and #1137
        mapping
            .insert("*.conf", MappingTarget::MapExtensionToUnknown)
            .unwrap();

        for glob in &[
            "/etc/nginx/**/*.conf",
            "/etc/nginx/sites-*/**/*",
            "nginx.conf",
            "mime.types",
        ] {
            mapping.insert(glob, MappingTarget::MapTo("nginx")).unwrap();
        }

        for glob in &[
            "/etc/apache2/**/*.conf",
            "/etc/apache2/sites-*/**/*",
            "httpd.conf",
        ] {
            mapping
                .insert(glob, MappingTarget::MapTo("Apache Conf"))
                .unwrap();
        }

        for glob in &[
            "**/systemd/**/*.conf",
            "**/systemd/**/*.example",
            "*.automount",
            "*.device",
            "*.dnssd",
            "*.link",
            "*.mount",
            "*.netdev",
            "*.network",
            "*.nspawn",
            "*.path",
            "*.service",
            "*.scope",
            "*.slice",
            "*.socket",
            "*.swap",
            "*.target",
            "*.timer",
        ] {
            mapping.insert(glob, MappingTarget::MapTo("INI")).unwrap();
        }

        // pacman hooks
        mapping
            .insert("*.hook", MappingTarget::MapTo("INI"))
            .unwrap();

        if let Some(xdg_config_home) = std::env::var_os("XDG_CONFIG_HOME") {
            let git_config_path = Path::new(&xdg_config_home).join("git");

            mapping
                .insert(
                    &git_config_path.join("config").to_string_lossy(),
                    MappingTarget::MapTo("Git Config"),
                )
                .ok();

            mapping
                .insert(
                    &git_config_path.join("ignore").to_string_lossy(),
                    MappingTarget::MapTo("Git Ignore"),
                )
                .ok();

            mapping
                .insert(
                    &git_config_path.join("attributes").to_string_lossy(),
                    MappingTarget::MapTo("Git Attributes"),
                )
                .ok();
        }

        mapping
    }

    pub fn insert(&mut self, from: &str, to: MappingTarget<'a>) -> Result<()> {
        let glob = GlobBuilder::new(from)
            .case_insensitive(false)
            .literal_separator(true)
            .build()?;
        self.mappings.push((glob.compile_matcher(), to));
        Ok(())
    }

    pub fn mappings(&self) -> &[(GlobMatcher, MappingTarget<'a>)] {
        &self.mappings
    }

    pub(crate) fn get_syntax_for(&self, path: impl AsRef<Path>) -> Option<MappingTarget<'a>> {
        let candidate = Candidate::new(&path);
        let candidate_filename = path.as_ref().file_name().map(Candidate::new);
        for (ref glob, ref syntax) in self.mappings.iter().rev() {
            if glob.is_match_candidate(&candidate)
                || candidate_filename
                    .as_ref()
                    .map_or(false, |filename| glob.is_match_candidate(filename))
            {
                return Some(*syntax);
            }
        }
        None
    }

    pub fn insert_ignored_suffix(&mut self, suffix: &'a str) {
        self.ignored_suffixes.add_suffix(suffix);
    }
}

#[test]
fn basic() {
    let mut map = SyntaxMapping::empty();
    map.insert("/path/to/Cargo.lock", MappingTarget::MapTo("TOML"))
        .ok();
    map.insert("/path/to/.ignore", MappingTarget::MapTo("Git Ignore"))
        .ok();

    assert_eq!(
        map.get_syntax_for("/path/to/Cargo.lock"),
        Some(MappingTarget::MapTo("TOML"))
    );
    assert_eq!(map.get_syntax_for("/path/to/other.lock"), None);

    assert_eq!(
        map.get_syntax_for("/path/to/.ignore"),
        Some(MappingTarget::MapTo("Git Ignore"))
    );
}

#[test]
fn user_can_override_builtin_mappings() {
    let mut map = SyntaxMapping::builtin();

    assert_eq!(
        map.get_syntax_for("/etc/profile"),
        Some(MappingTarget::MapTo("Bourne Again Shell (bash)"))
    );
    map.insert("/etc/profile", MappingTarget::MapTo("My Syntax"))
        .ok();
    assert_eq!(
        map.get_syntax_for("/etc/profile"),
        Some(MappingTarget::MapTo("My Syntax"))
    );
}

#[test]
fn builtin_mappings() {
    let map = SyntaxMapping::builtin();

    assert_eq!(
        map.get_syntax_for("/path/to/build"),
        Some(MappingTarget::MapToUnknown)
    );
}
