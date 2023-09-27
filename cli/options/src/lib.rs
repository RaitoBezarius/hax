use clap::{Parser, Subcommand, ValueEnum};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};

pub use hax_frontend_exporter_options::*;

#[derive(JsonSchema, Debug, Clone, Serialize, Deserialize)]
pub struct ForceCargoBuild {
    pub data: u128,
}

impl std::default::Default for ForceCargoBuild {
    fn default() -> Self {
        ForceCargoBuild { data: 0 }
    }
}

impl std::convert::From<&std::ffi::OsStr> for ForceCargoBuild {
    fn from(s: &std::ffi::OsStr) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        if s == "false" {
            ForceCargoBuild {
                data: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|r| r.as_millis())
                    .unwrap_or(0),
            }
        } else {
            ForceCargoBuild::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum PathOrDash {
    Dash,
    Path(PathBuf),
}

impl std::convert::From<&std::ffi::OsStr> for PathOrDash {
    fn from(s: &std::ffi::OsStr) -> Self {
        if s == "-" {
            PathOrDash::Dash
        } else {
            PathOrDash::Path(PathBuf::from(s))
        }
    }
}

impl PathOrDash {
    pub fn open_or_stdout(&self) -> Box<dyn std::io::Write> {
        use std::io::BufWriter;
        match self {
            PathOrDash::Dash => Box::new(BufWriter::new(std::io::stdout())),
            PathOrDash::Path(path) => {
                Box::new(BufWriter::new(std::fs::File::create(&path).unwrap()))
            }
        }
    }
    pub fn map_path<F: FnOnce(&Path) -> PathBuf>(&self, f: F) -> Self {
        match self {
            PathOrDash::Path(path) => PathOrDash::Path(f(path)),
            PathOrDash::Dash => PathOrDash::Dash,
        }
    }
}

fn absolute_path(path: impl AsRef<std::path::Path>) -> std::io::Result<std::path::PathBuf> {
    use path_clean::PathClean;
    let path = path.as_ref();

    let absolute_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    }
    .clean();

    Ok(absolute_path)
}

pub trait NormalizePaths {
    fn normalize_paths(self) -> Self;
}

impl NormalizePaths for PathBuf {
    fn normalize_paths(self) -> Self {
        absolute_path(self).unwrap()
    }
}
impl NormalizePaths for PathOrDash {
    fn normalize_paths(self) -> Self {
        match self {
            PathOrDash::Dash => PathOrDash::Dash,
            PathOrDash::Path(p) => PathOrDash::Path(p.normalize_paths()),
        }
    }
}

#[derive(JsonSchema, Subcommand, Debug, Clone, Serialize, Deserialize)]
pub enum Backend {
    /// Use the F* backend
    Fstar,
    /// Use the Coq backend
    Coq,
    /// Use the EasyCrypt backend
    Easycrypt,
}

impl fmt::Display for Backend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Backend::Fstar => write!(f, "fstar"),
            Backend::Coq => write!(f, "coq"),
            Backend::Easycrypt => write!(f, "easycrypt"),
        }
    }
}

#[derive(JsonSchema, Debug, Clone, Serialize, Deserialize)]
enum InclusionKind {
    Included,
    Excluded,
}

#[derive(JsonSchema, Debug, Clone, Serialize, Deserialize)]
struct InclusionClause {
    kind: InclusionKind,
    namespace: Namespace,
}

fn parse_inclusion_clause(
    s: &str,
) -> Result<InclusionClause, Box<dyn std::error::Error + Send + Sync + 'static>> {
    let s = s.trim();
    if s.is_empty() {
        Err("Expected `-` or `+`, got an empty string")?
    }
    let (prefix, namespace) = s.split_at(1);
    let kind = match prefix {
        "+" => InclusionKind::Included,
        "-" => InclusionKind::Excluded,
        prefix => Err(format!("Expected `-` or `+`, got an `{prefix}`"))?,
    };
    Ok(InclusionClause {
        kind,
        namespace: namespace.to_string().into(),
    })
}

#[derive(JsonSchema, Parser, Debug, Clone, Serialize, Deserialize)]
pub struct TranslationOptions {
    /// Space-separated list of inclusion clauses. An inclusion clause
    /// is a Rust path prefixed with either `+` or `-`. By default,
    /// every item is included. Rust path chunks can be either a
    /// concrete string, `*` or `**`. The two latter are globs.
    #[arg(
        value_parser = parse_inclusion_clause,
        value_delimiter = ' ',
    )]
    #[arg(short, allow_hyphen_values(true))]
    include_namespaces: Vec<InclusionClause>,
}

#[derive(JsonSchema, Parser, Debug, Clone, Serialize, Deserialize)]
pub struct BackendOptions {
    #[command(subcommand)]
    pub backend: Backend,

    /// Don't write anything on disk. Output everything as JSON to stdout
    /// instead.
    #[arg(long = "dry-run")]
    pub dry_run: bool,

    /// Verbose mode for the Hax engine. Set [-vv] for maximal verbosity.
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Enable debugging of the engine, and visualize interactively in
    /// a webapp how a crate was transformed by each phase, both in
    /// Rust-like syntax and browsing directly the internal AST. By
    /// default, the webapp is hosted on http://localhost:8000, the
    /// port can be override by setting the `HAX_DEBUGGER_PORT`
    /// environment variable.
    #[arg(short, long = "debug-engine")]
    debug_engine: bool,

    #[command(flatten)]
    translation_options: TranslationOptions,
}

#[derive(JsonSchema, Subcommand, Debug, Clone, Serialize, Deserialize)]
pub enum ExporterCommand {
    /// Translate to a backend. The translated modules will be written
    /// under the directory [<PKG>/proofs/<BACKEND>/extraction], where
    /// <PKG> is the translated cargo package name and <BACKEND> the
    /// name of the backend.
    #[clap(name = "into")]
    Backend(BackendOptions),

    /// Export directly as a JSON file
    JSON {
        /// Path to the output JSON file, "-" denotes stdout.
        #[arg(
            short,
            long = "output-file",
            default_value = "hax_frontend_export.json"
        )]
        output_file: PathOrDash,
        /// Wether the bodies are exported as THIR, built MIR, const
        /// MIR, or a combination. Repeat this option to extract a
        /// combination (e.g. [-k thir -k mir-built]).
        #[arg(
            value_enum,
            short,
            long = "kind",
            num_args = 0..=3,
            default_values_t = [ExportBodyKind::Thir]
        )]
        kind: Vec<ExportBodyKind>,
    },
}

#[derive(
    JsonSchema, ValueEnum, Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord,
)]
pub enum ExportBodyKind {
    Thir,
    MirBuilt,
    MirConst,
}

#[derive(JsonSchema, Subcommand, Debug, Clone, Serialize, Deserialize)]
pub enum LinterCommand {
    /// Lint for the hacspec subset
    Hacspec,
    /// Lint for the supported Rust subset
    Rust,
}

#[derive(JsonSchema, Subcommand, Debug, Clone, Serialize, Deserialize)]
pub enum RustcCommand {
    #[command(flatten)]
    ExporterCommand(ExporterCommand),
    /// Lint the code
    #[clap(subcommand, name = "lint")]
    LintCommand(LinterCommand),
}

#[derive(JsonSchema, Subcommand, Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    #[command(flatten)]
    RustcCommand(RustcCommand),
    /// Check the proofs
    #[command(subcommand, name = "check")]
    CheckCommand(Backend),
}

#[derive(JsonSchema, Parser, Debug, Clone, Serialize, Deserialize)]
#[command(author, version = concat!("commit=", env!("HAX_GIT_COMMIT_HASH"), " ", "describe=", env!("HAX_GIT_DESCRIBE")), name = "hax", about, long_about = None)]
pub struct Options<Command: clap::Subcommand> {
    /// Replace the expansion of each macro matching PATTERN by their
    /// invocation. PATTERN denotes a rust path (i.e. [A::B::c]) in
    /// which glob patterns are allowed. The glob pattern * matches
    /// any name, the glob pattern ** matches zero, one or more
    /// names. For instance, [A::B::C::D::X] and [A::E::F::D::Y]
    /// matches [A::**::D::*].
    #[arg(
        short = 'i',
        long = "inline-macro-call",
        value_name = "PATTERN",
        value_parser,
        value_delimiter = ',',
        default_values = [
            "hacspec_lib::array::array", "hacspec_lib::array::public_bytes", "hacspec_lib::array::bytes",
            "hacspec_lib::math_integers::public_nat_mod", "hacspec_lib::math_integers::unsigned_public_integer",
        ],
    )]
    pub inline_macro_calls: Vec<Namespace>,

    /// Semi-colon terminated list of arguments to pass to the
    /// [cargo build] invocation. For example, to apply this
    /// program on a package [foo], use [-C -p foo ;]. (make sure
    /// to escape [;] correctly in your shell)
    #[arg(default_values = Vec::<&str>::new(), short='C', allow_hyphen_values=true, num_args=1.., long="cargo-args", value_terminator=";")]
    pub cargo_flags: Vec<String>,

    #[command(subcommand)]
    pub command: Command,

    /// [cargo] caching is disabled by default, this flag enables it back.
    #[arg(long="enable-cargo-cache", action=clap::builder::ArgAction::SetTrue)]
    pub force_cargo_build: ForceCargoBuild,

    /// Apply the command to every local package of the dependency closure. By
    /// default, the command is only applied to the primary packages (i.e. the
    /// package(s) of the current directory, or the ones selected with cargo
    /// options like [-C -p <PKG> ;]).
    #[arg(long = "deps")]
    pub deps: bool,
}

impl NormalizePaths for ExporterCommand {
    fn normalize_paths(self) -> Self {
        use ExporterCommand::*;
        match self {
            JSON { output_file, kind } => JSON {
                output_file: output_file.normalize_paths(),
                kind,
            },
            Backend(o) => Backend(o),
        }
    }
}

impl NormalizePaths for RustcCommand {
    fn normalize_paths(self) -> Self {
        match self {
            RustcCommand::ExporterCommand(cmd) => {
                RustcCommand::ExporterCommand(cmd.normalize_paths())
            }
            _ => self,
        }
    }
}

impl NormalizePaths for Command {
    fn normalize_paths(self) -> Self {
        match self {
            Command::RustcCommand(cmd) => Command::RustcCommand(cmd.normalize_paths()),
            _ => self,
        }
    }
}

impl<Command: NormalizePaths + clap::Subcommand> NormalizePaths for Options<Command> {
    fn normalize_paths(self) -> Self {
        Self {
            command: self.command.normalize_paths(),
            ..self
        }
    }
}

impl From<Options<RustcCommand>> for hax_frontend_exporter_options::Options {
    fn from(opts: Options<RustcCommand>) -> Self {
        hax_frontend_exporter_options::Options {
            inline_macro_calls: opts.inline_macro_calls,
        }
    }
}

pub const ENV_VAR_OPTIONS_FRONTEND: &str = "DRIVER_HAX_FRONTEND_OPTS";
