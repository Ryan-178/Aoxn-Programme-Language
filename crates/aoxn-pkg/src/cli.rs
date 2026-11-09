//! Command-line surface (clap). The `aoxn` binary forwards its package
//! subcommands here with the dispatch token stripped.

use clap::Parser as _;

use crate::errors::PkgError;

#[derive(clap::Parser, Debug)]
#[command(
    name = "aoxn-pkg",
    about = "Aoxn package manager",
    disable_help_subcommand = true,
    after_help = "Run `aoxn <command> --help` for per-command flags."
)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Cmd,

    /// show what would happen without changing anything
    #[arg(long, global = true)]
    pub dry_run: bool,
    /// print the full derivation on resolution conflicts
    #[arg(long, global = true)]
    pub explain: bool,
    /// work only from the local cache; never touch the network
    #[arg(long, global = true)]
    pub offline: bool,
    /// disable colored output (also automatic when not a TTY, or NO_COLOR set)
    #[arg(long, global = true)]
    pub no_color: bool,
    /// machine-readable output where supported
    #[arg(long, global = true)]
    pub json: bool,
    /// suppress progress chatter
    #[arg(long, global = true)]
    pub quiet: bool,
}

#[derive(clap::Subcommand, Debug)]
pub enum Cmd {
    /// create aoxn.toml (and a hello-world main) in the current directory
    Init {
        /// default registry URL to record in the manifest
        #[arg(long)]
        registry: Option<String>,
        /// package name (default: current directory name)
        #[arg(long)]
        name: Option<String>,
    },
    /// alias for `aoxn install <pkg>` (records the dependency, then installs)
    Add {
        /// packages to add, e.g. `http>=1.2`, `http ^2`, `local @ path:../local`
        pkgs: Vec<String>,
        /// add to the `dev` extra instead of runtime dependencies
        #[arg(short = 'D', long)]
        dev: bool,
        /// bind the dependency to a named registry from aoxn.toml `registries`
        #[arg(long)]
        registry: Option<String>,
    },
    /// convert a legacy aoxn.json project to aoxn.toml
    Migrate {
        /// keep aoxn.json next to the new aoxn.toml
        #[arg(long)]
        keep: bool,
    },
    /// remove dependency(ies) from the manifest
    Remove {
        /// package names to remove
        pkgs: Vec<String>,
        /// remove from devDependencies only
        #[arg(short = 'D', long)]
        dev: bool,
    },
    /// install per aoxn.toml / aoxn.lock; with package arguments, record
    /// them in the manifest first (pip-style `aoxn install http>=1.2`)
    Install {
        /// packages to declare and install, e.g. `http>=1.2` or `./local-lib`
        pkgs: Vec<String>,
        /// record the packages in the `dev` extra (with no packages: install
        /// dev dependencies only)
        #[arg(short = 'D', long)]
        dev: bool,
        /// registry for packages being declared
        #[arg(long)]
        registry: Option<String>,
        /// re-resolve even when the lockfile matches the manifests
        #[arg(long)]
        force: bool,
        /// CI mode: fail when the lockfile does not exactly satisfy the
        /// manifests instead of re-resolving (no lockfile updates ever)
        #[arg(long)]
        frozen: bool,
        /// production only: leave dev-only packages out of aox_modules/
        #[arg(long, conflicts_with = "dev_only")]
        prod: bool,
        /// dev tooling only: install dev dependencies and skip the rest
        #[arg(long)]
        dev_only: bool,
        /// parallel tarball downloads (default 8; 1 = serial)
        #[arg(long)]
        jobs: Option<usize>,
    },
    /// upgrade dependencies within their constraints (--latest crosses majors)
    Update {
        /// specific packages to update (default: all)
        pkgs: Vec<String>,
        /// bump the manifest constraint to the newest version
        #[arg(long)]
        latest: bool,
        /// operate on devDependencies
        #[arg(short = 'D', long)]
        dev: bool,
        /// parallel tarball downloads (default 8; 1 = serial)
        #[arg(long)]
        jobs: Option<usize>,
    },
    /// list outdated dependencies
    Outdated,
    /// list installed packages (name, version, scope, source)
    List {
        /// production packages only
        #[arg(long, conflicts_with = "dev_only")]
        prod: bool,
        /// dev-only packages only
        #[arg(long)]
        dev_only: bool,
    },
    /// print installed packages as `name==version` lines (pip freeze)
    Freeze {
        /// leave dev-only packages out
        #[arg(long)]
        prod: bool,
    },
    /// print the installed dependency tree
    Tree {
        /// limit the depth shown (default: unlimited)
        #[arg(long)]
        depth: Option<usize>,
    },
    /// show why a package is installed (all requirement paths)
    Why { pkg: String },
    /// publish this package to the registry (idempotent, tags `pkg/<name>/v<ver>`)
    Publish {
        /// publish even with uncommitted changes in the package's git repo
        #[arg(long)]
        allow_dirty: bool,
    },
    /// mark a published version as yanked (fresh resolution refuses it;
    /// existing lockfiles keep working). Undo with --undo.
    Yank {
        pkg: String,
        version: String,
        #[arg(long)]
        undo: bool,
    },
    /// check the lockfile against the advisory database
    Audit {
        /// only fail on advisories at or above this severity
        /// (low | medium | high | critical)
        #[arg(long)]
        audit_level: Option<String>,
        /// raise manifest constraints to the advisory's patched version,
        /// but only where that does not widen a range on purpose
        #[arg(long)]
        fix: bool,
    },
    /// curated-registry trust: wire it up, list it, or gate on it
    Trust {
        #[command(subcommand)]
        sub: TrustCmd,
    },
    /// remove dependencies and clean up aox_modules (the pip spelling)
    Uninstall {
        /// package names to remove
        pkgs: Vec<String>,
        /// remove from the `dev` extra only
        #[arg(short = 'D', long)]
        dev: bool,
    },
    /// build all workspace members in dependency order
    Build,
    /// import an Aoxn package published to an npm-compatible registry
    /// (one-shot bridge; the npm CLI is the transport)
    NpmImport {
        /// npm specs to import: `my-pkg`, `my-pkg@1.2.0`, `@scope/pkg@^1`
        specs: Vec<String>,
        /// import every dependency of an npm package.json that is an Aoxn package
        #[arg(long)]
        from: Option<String>,
    },
    /// manage the global cache
    Cache {
        #[command(subcommand)]
        sub: CacheCmd,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum TrustCmd {
    /// point the global config at a curated registry: default registry,
    /// advisory database and trust index, all from one URL
    Bootstrap {
        /// curated registry URL (defaults to the already configured one)
        #[arg(long)]
        registry: Option<String>,
    },
    /// list the packages a curated registry has reviewed
    List,
    /// exit non-zero unless the package is reviewed at the required tier
    Check {
        /// package name
        pkg: String,
        /// minimum review tier (unreviewed | community | audited)
        #[arg(long, default_value = "community")]
        tier: String,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum CacheCmd {
    /// print the cache directory
    Dir,
    /// delete cached tarballs not referenced by any aoxn.lock under the cwd
    Gc,
    /// drop cached registry snapshots (re-fetched on demand)
    Prune,
    /// delete the entire cache
    Clean,
    /// print total cache size
    Size,
}

/// Parse and run. Returns the process exit code (help/version/errors from
/// clap print themselves and exit via `e.exit()`).
pub fn dispatch(args: &[String]) -> Result<i32, PkgError> {
    // clap treats argv[0] as the program name; the caller strips the
    // dispatch token (`pkg`/`add`/...) so prepend a synthetic one
    let argv = std::iter::once("aoxn".to_string())
        .chain(args.iter().cloned())
        .collect::<Vec<_>>();
    let cli = match Cli::try_parse_from(argv) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };
    crate::ui::init(cli.no_color, cli.json, cli.quiet);
    run_command(cli)
}

fn run_command(cli: Cli) -> Result<i32, PkgError> {
    let explain = cli.explain;
    let res: Result<i32, PkgError> = match cli.cmd {
        Cmd::Init { registry, name } => crate::manage::init(registry, name),
        Cmd::Add { pkgs, dev, registry } => {
            crate::manage::add(&pkgs, dev, registry.as_deref(), cli.dry_run, cli.offline)
        }
        Cmd::Migrate { keep } => crate::manage::migrate(keep, cli.dry_run),
        Cmd::Remove { pkgs, dev } => crate::manage::remove(&pkgs, dev, cli.dry_run, cli.offline),
        Cmd::Uninstall { pkgs, dev } => crate::manage::remove(&pkgs, dev, cli.dry_run, cli.offline),
        Cmd::Install { pkgs, dev, registry, force, frozen, prod, dev_only, jobs } => {
            if pkgs.is_empty() {
                if registry.is_some() {
                    return Err(PkgError::Manifest(
                        "`--registry` needs a package argument".into(),
                    ));
                }
                if dev && (prod || dev_only) {
                    return Err(PkgError::Manifest(
                        "`--dev` means dev-only here and cannot be combined with --prod/--dev-only"
                            .into(),
                    ));
                }
                crate::manage::install_cmd(
                    force,
                    frozen,
                    crate::manage::scope_from_flags(prod, dev_only || dev),
                    crate::manage::jobs_from_flag(jobs),
                    cli.dry_run,
                    cli.offline,
                    cli.json,
                )
            } else {
                if frozen {
                    return Err(PkgError::Manifest(
                        "`--frozen` cannot be combined with package arguments: declaring a \
                         dependency necessarily changes the manifest"
                            .into(),
                    ));
                }
                if prod || dev_only {
                    return Err(PkgError::Manifest(
                        "package arguments cannot be combined with --prod/--dev-only; \
                         use --dev to record a dev dependency"
                            .into(),
                    ));
                }
                crate::manage::install_packages(
                    &pkgs,
                    dev,
                    registry.as_deref(),
                    crate::manage::jobs_from_flag(jobs),
                    cli.dry_run,
                    cli.offline,
                    cli.json,
                )
            }
        }
        Cmd::Update { pkgs, latest, dev, jobs } => crate::manage::update(
            &pkgs,
            latest,
            dev,
            cli.dry_run,
            cli.offline,
            crate::manage::jobs_from_flag(jobs),
        ),
        Cmd::Outdated => crate::outdated::run(cli.offline, cli.json),
        Cmd::List { prod, dev_only } => crate::list::run(
            crate::manage::scope_from_flags(prod, dev_only),
            cli.json,
        ),
        Cmd::Freeze { prod } => crate::list::freeze(if prod {
            crate::context::DepScope::Prod
        } else {
            crate::context::DepScope::All
        }),
        Cmd::Tree { depth } => crate::tree::run_tree(depth, cli.json),
        Cmd::Why { pkg } => crate::tree::run_why(&pkg, cli.json),
        Cmd::Publish { allow_dirty } => crate::publish::run(allow_dirty, cli.dry_run),
        Cmd::Yank { pkg, version, undo } => crate::publish::yank(&pkg, &version, undo, cli.dry_run),
        Cmd::Audit { audit_level, fix } => {
            crate::audit::run(audit_level.as_deref(), cli.json, fix, cli.offline)
        }
        Cmd::Trust { sub } => match sub {
            TrustCmd::Bootstrap { registry } => crate::trust::bootstrap(registry, cli.offline),
            TrustCmd::List => crate::trust::list(cli.json),
            TrustCmd::Check { pkg, tier } => {
                let min = crate::trust::Tier::from_str(&tier)?;
                crate::trust::check(&pkg, min)
            }
        },
        Cmd::Build => crate::publish::build(),
        Cmd::NpmImport { specs, from } => crate::npm::import(&specs, from.as_deref(), cli.dry_run),
        Cmd::Cache { sub } => crate::cache_cmd::run(sub),
    };
    match res {
        Ok(code) => Ok(code),
        Err(e) => {
            if explain {
                if let Some(detail) = e.explain() {
                    eprintln!();
                    eprintln!("{}", crate::ui::style_hint("── explain ─────────────────────────"));
                    eprintln!("{detail}");
                }
            }
            crate::errors::report(&e);
            Ok(1)
        }
    }
}
