#![doc = include_str!("../README.md")]
//!
//! # Example
//!
//! In simple cases you can just call `BinTest::new()` to build all executables in the current
//! crate and get a reference to a `BinTest` singleton to work with.
//!
//! ```rust
//! #[test]
//! fn test() {
//!   // BinTest::new() will run 'cargo build' and registers all build executables
//!   let executables: &'static BinTest = BinTest::new();
//!
//!   // List the executables build
//!   for (k,v) in executables.list_executables() {
//!     println!("{} @ {}", k, v);
//!   }
//!
//!   // BinTest::command() looks up executable by its name and creates a process::Command from it
//!   let command = executables.command("name");
//!
//!   // this command can then be used for testing
//!   command.arg("help").spawn();
//! }
//! ```
//!
//!
//! # See Also
//!
//! The 'testcall' crate uses this to build tests and assertions on top of the commands
//! created by bintest. The 'testpath' crate lets you run test in specially created temporary
//! directories to provide an filesystem environment for tests.
use std::env::var_os as env;
use std::ffi::OsString;
use std::{collections::BTreeMap, sync::OnceLock};

pub use std::process::{Command, Stdio};

pub use cargo_metadata::camino::Utf8PathBuf;
use cargo_metadata::Message;

/// Allows configuration of a workspace to find an executable in.
///
/// This builder is completely const constructible.
#[must_use]
#[derive(Debug, PartialEq, Clone, Copy)]
pub struct BinTestBuilder {
    build_workspace: bool,
    specific_executable: Option<&'static str>,
    quiet: bool,
}

/// Access to binaries build by 'cargo build'
#[derive(Debug)]
pub struct BinTest {
    configured_with: BinTestBuilder,
    build_executables: BTreeMap<String, Utf8PathBuf>,
}

//PLANNED: needs some better way to figure out what profile is active
#[cfg(not(debug_assertions))]
const RELEASE_BUILD: bool = true;

#[cfg(debug_assertions)]
const RELEASE_BUILD: bool = false;

impl BinTestBuilder {
    /// Constructs a default builder that does not build workspace executables
    const fn new() -> BinTestBuilder {
        Self {
            build_workspace: false,
            specific_executable: None,
            quiet: false,
        }
    }

    /// Allow building all executables in a workspace
    pub const fn build_workspace(self, workspace: bool) -> Self {
        Self {
            build_workspace: workspace,
            ..self
        }
    }

    /// Allow only building a specific executable in the case of multiple in a workspace/package
    pub const fn build_executable(self, executable: &'static str) -> Self {
        Self {
            specific_executable: Some(executable),
            ..self
        }
    }

    /// Allow disabling extra output from the `cargo build` run
    pub const fn quiet(self, quiet: bool) -> Self {
        Self { quiet, ..self }
    }

    /// Constructs a `BinTest` with the default configuration if not already constructed.
    /// Construction runs 'cargo build' and register all build executables.  Executables are
    /// identified by their name, without path and filename extension.
    ///
    /// # Returns
    ///
    /// A reference to a immutable `BinTest` singleton that can be used to access the
    /// executables.
    ///
    /// # Panics
    ///
    /// All tests must run with the same configuration, this can be either achieved by calling
    /// `BinTest::with()` always with the same configuration or by providing a function that
    /// constructs and returns the `BinTest` singleton:
    ///
    /// ```
    /// use bintest::{BinTest, BinTestBuilder};
    ///
    /// // #[cfg(test)]
    /// fn my_bintest() -> &'static BinTest {
    ///     // The Builder can be all const constructed
    ///     static BINTEST_CONFIG: BinTestBuilder = BinTest::with().quiet(true);
    ///     BINTEST_CONFIG.build()
    /// }
    ///
    /// // #[test]
    /// fn example() {
    ///     let bintest = my_bintest();
    /// }
    /// ```
    #[must_use]
    pub fn build(self) -> &'static BinTest {
        BinTest::new_with_builder(&self)
    }
}

impl BinTest {
    /// Creates a `BinTestBuilder` for further customization.
    ///
    /// # Example
    ///
    /// ```
    /// use bintest::BinTest;
    ///
    /// let executables = BinTest::with().quiet(true).build();
    /// ```
    pub const fn with() -> BinTestBuilder {
        BinTestBuilder::new()
    }

    /// Constructs a `BinTest` with the default configuration if not already constructed.
    /// Construction runs 'cargo build' and register all build executables.  Executables are
    /// identified by their name, without path and filename extension.
    ///
    /// # Returns
    ///
    /// A reference to a immutable `BinTest` singleton that can be used to access the
    /// executables.
    ///
    /// # Panics
    ///
    /// All tests must run with the same configuration, when using only `BinTest::new()` this
    /// is infallible. Mixing this with differing configs from `BinTest::with()` will panic.
    #[must_use]
    pub fn new() -> &'static Self {
        Self::new_with_builder(&BinTestBuilder::new())
    }

    /// Gives an `(name, path)` iterator over all executables found
    pub fn list_executables(&self) -> std::collections::btree_map::Iter<'_, String, Utf8PathBuf> {
        self.build_executables.iter()
    }

    /// Constructs a `std::process::Command` for the given executable name
    #[must_use]
    pub fn command(&self, name: &str) -> Command {
        Command::new(
            self.build_executables
                .get(name)
                .unwrap_or_else(|| panic!("no such executable <<{name}>>")),
        )
    }

    fn new_with_builder(builder: &BinTestBuilder) -> &'static Self {
        static SINGLETON: OnceLock<BinTest> = OnceLock::new();

        let singleton = SINGLETON.get_or_init(|| {
            let mut cargo_build =
                Command::new(env("CARGO").unwrap_or_else(|| OsString::from("cargo")));

            cargo_build
                .args(["build", "--message-format", "json"])
                .stdout(Stdio::piped());

            if RELEASE_BUILD {
                cargo_build.arg("--release");
            }

            if builder.build_workspace {
                cargo_build.arg("--workspace");
            }

            if let Some(executable) = builder.specific_executable {
                cargo_build.args(["--bin", &executable]);
            }

            if builder.quiet {
                cargo_build.arg("--quiet");
            }

            let mut cargo_result = cargo_build.spawn().expect("'cargo build' success");

            let mut build_executables = BTreeMap::<String, Utf8PathBuf>::default();

            let reader = std::io::BufReader::new(cargo_result.stdout.take().unwrap());
            for message in cargo_metadata::Message::parse_stream(reader) {
                if let Message::CompilerArtifact(artifact) = message.unwrap() {
                    if let Some(executable) = artifact.executable {
                        build_executables.insert(
                            String::from(executable.file_stem().expect("filename")),
                            executable.to_path_buf(),
                        );
                    }
                }
            }

            BinTest {
                configured_with: *builder,
                build_executables,
            }
        });

        assert_eq!(
            singleton.configured_with, *builder,
            "All instances of BinTest must be configured with the same values"
        );

        singleton
    }
}

// The following tests are mutually exclusive since we operate on a global singleton

// #[test]
// fn same_config() {
//     let _executables1 = BinTest::with().build_workspace(true).build();
//     let _executables2 = BinTest::with().build_workspace(true).build();
// }

#[test]
#[should_panic(expected = "All instances of BinTest must be configured with the same values")]
fn different_config() {
    let _executables1 = BinTest::new();
    let _executables2 = BinTest::with().build_workspace(true).build();
}
