//! Put some fancy documentation here.
//!
//! ```toml
//! [package.metadata.buildkit]
//! vendored-source = "..."
//! pkg-config = "..."
//! vcpkg = "..."
//! ```

use camino::Utf8PathBuf;
use cargo_metadata::MetadataCommand;
use serde::Deserialize;

/// This will be the builder pattern thing that people interact with in their build.rs
pub struct BuildKit {
    metadata: BuildKitMetadata,
}

impl BuildKit {
    /// Collects information from the `package.metadata.buildkit`
    /// section of the Cargo.toml file for the package being built.
    pub fn from_metadata() -> Result<Self, Error> {
        let manifest_dir = env_var("CARGO_MANIFEST_DIR").map(Utf8PathBuf::from)?;
        let manifest_path = manifest_dir.join("Cargo.toml");
        let metadata = MetadataCommand::new()
            .manifest_path(&manifest_path)
            .no_deps()
            .exec()
            .map_err(ErrorKind::CargoMetadataError)?;

        let name = env_var("CARGO_PKG_NAME")?;
        let version = env_var("CARGO_PKG_VERSION")?;
        let package = metadata
            .packages
            .iter()
            .filter(|p| p.name == name && p.version.to_string() == version)
            .next()
            .ok_or_else(|| {
                ErrorKind::InvalidCargoMetadata(format!("package info from {name}@{version}"))
            })?;
        let value = package
            .metadata
            .get("buildkit")
            .ok_or_else(|| {
                ErrorKind::InvalidCargoMetadata(format!("metadata.buildkit for {name}@{version}"))
            })?
            .clone();
        let metadata = serde_json::from_value(value).map_err(ErrorKind::Json)?;
        Ok(BuildKit { metadata })
    }

    /// Builds the library.
    ///
    /// The `try_vendor` closure is for building from vendoered source
    /// if the `package.metadata.buildkit.vendored-source` section is specified.
    pub fn build<F>(&self, try_vendor: F) -> Result<(), Error>
    where
        F: Fn(VendoredBuildContext) -> Result<(), Error>,
    {
        match self.mode()? {
            BuildKitMode::VendoredBuild => {
                let vendored_source = self
                    .metadata
                    .vendored_source
                    .as_ref()
                    .ok_or_else(|| ErrorKind::NoVendoredSourceSpecified)?;
                let ctx = VendoredBuildContext::new(vendored_source);
                try_vendor(ctx)
            }
            BuildKitMode::PkgConfig => {
                let req = self
                    .metadata
                    .pkg_config
                    .as_ref()
                    .ok_or_else(|| ErrorKind::NoPkgConfigRequirementSpecified)?;
                try_pkg_config(req)
            }
            BuildKitMode::Vcpkg => {
                let req = self
                    .metadata
                    .vcpkg
                    .as_ref()
                    .ok_or_else(|| ErrorKind::NoVcpkgRequirementSpecified)?;
                try_vcpkg(req)
            }
        }
    }

    /// Gets the mode we're going to build in.
    ///
    /// TODO: ways for external build systems to override
    fn mode(&self) -> Result<BuildKitMode, Error> {
        if matches!(self.metadata.default_mode, BuildKitMode::VendoredBuild) {
            return Ok(BuildKitMode::VendoredBuild);
        }
        let target = env_var("TARGET")?;
        // TODO: should we relax it to `-windows-`?
        // Some people seems to use vcpkg with mingw: https://www.reddit.com/r/cpp/comments/p1655e/comment/h8bly7v
        //
        // TODO: should we retry if vcpkg found nothing?
        // curl-sys falls back to pkg_config when vcpkg failed.
        // https://github.com/alexcrichton/curl-rust/blob/c01261310f13c85dc70d4e8a1ef87504662a1154/curl-sys/build.rs#L30-L37
        if target.ends_with("-windows-msvc") {
            Ok(BuildKitMode::Vcpkg)
        } else {
            Ok(BuildKitMode::PkgConfig)
        }
    }
}

fn env_var(key: &'static str) -> Result<String, Error> {
    std::env::var(key).map_err(|err| ErrorKind::EnvVarError { key, err }.into())
}

/// Represents possible errors that can occur when build libraries
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct Error(#[from] ErrorKind);

impl Error {
    /// Creates a custom error.
    ///
    /// This is useful during a vendor build and you want to return your own error.
    pub fn custom(err: Box<dyn std::error::Error>) -> Error {
        ErrorKind::Custom(err).into()
    }
}

/// Non-public error kind for [`Error`].
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
enum ErrorKind {
    #[error("Failed to parse cargo metadata: {0}")]
    CargoMetadataError(#[from] cargo_metadata::Error),

    #[error("Failed to find in cargo metadata output: {0}")]
    InvalidCargoMetadata(String),

    #[error("Failed to deserialize `package.metadata.buildkit`: {0}")]
    Json(#[from] serde_json::Error),

    #[error("vendored mode is set but no vendored source specified")]
    NoVendoredSourceSpecified,

    #[error("pkg-config mode is set but no pkg-config requirement specified")]
    NoPkgConfigRequirementSpecified,

    #[error("vcpkg mode is set but no vcpkg requirement specified")]
    NoVcpkgRequirementSpecified,

    #[error("vcpkg failed to probe: {0}")]
    VcpkgError(#[from] vcpkg::Error),

    #[error("pkg-config failed to probe: {0}")]
    PkgConfigError(#[from] pkg_config::Error),

    #[error("Failed to get env var `{key}`: {err}")]
    EnvVarError {
        key: &'static str,
        #[source]
        err: std::env::VarError,
    },

    #[error(transparent)]
    Custom(Box<dyn std::error::Error>),
}

// This will represent the data that folks can specify within their Cargo.toml
// libgit2: name + version range for pkg-config
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct BuildKitMetadata {
    pkg_config: Option<PkgConfigRequirement>,
    vcpkg: Option<VcpkgRequirement>,
    vendored_source: Option<VendoredSource>,
    default_mode: BuildKitMode,
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
enum BuildKitMode {
    PkgConfig,
    Vcpkg,
    VendoredBuild,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PkgConfigRequirement {
    /// Library to probe. The value will be verbatimly passed to `pkg-config`.
    ///
    /// For example, libcurl will be `libcurl`.
    name: String,
    version_req: Option<PkgConfigVersionReq>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(rename_all_fields = "kebab-case")]
#[serde(untagged)]
enum PkgConfigVersionReq {
    /// `[min..max)` (or `min..max` in Rust notation).
    Range { min: String, max: String },
    /// At least the given version.
    Min { min: String },
    /// At no newer than the given version.
    Max { max: String },
    /// At exactly the given version.
    Exact { exact: String },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct VcpkgRequirement {
    /// If no overrides have been selected,
    /// then the Vcpkg port name is the `<name>.lib` and the `<name>.dll`.
    name: String,
    /// Override the name of the library to look for if it differs from the package name.
    ///
    /// See [`vcpkg::Config::lib_names`] for more.
    libs: Vec<VcpkgLibName>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct VcpkgLibName {
    /// `<name>.lib`.
    lib_name: String,
    /// `<name>.dll`.
    dll_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(rename_all_fields = "kebab-case")]
enum VendoredSource {
    RemoteTarball {
        url: String,
        hash: String,
    },
    // TODO: Is just ref enough here? SHA1...
    GitRepo {
        url: String,
        git_ref: String,
        hash: String,
    },
    CratePath {
        relative_path: Utf8PathBuf,
    },
    // TODO: Cannot be specified in the crate, only can be set at build time
    SystemPath {
        path: Utf8PathBuf,
    },
}

/// Provides the information needed for build a library from a vendred source.
#[derive(Debug)]
pub struct VendoredBuildContext {
    source_path: Utf8PathBuf,
}

impl VendoredBuildContext {
    fn new(source: &VendoredSource) -> VendoredBuildContext {
        VendoredBuildContext {
            source_path: Utf8PathBuf::new(),
        }
    }

    /// Gets the local path to the vendored source.
    pub fn source_path(&self) -> &Utf8PathBuf {
        &self.source_path
    }
}

/// Probes system libraries via the [`vcpkg`] crate.
///
/// As of `vcpkg@0.2.15`,
/// it appears that this crate doesn't really call into the [`vcpkg` from Microsoft][ms-vcpkg].
///
/// [ms-vcpkg]: https://github.com/microsoft/vcpkg
fn try_vcpkg(req: &VcpkgRequirement) -> Result<(), Error> {
    let name = req.name.as_str();
    emit_no_vendor(name);
    let mut config = vcpkg::Config::new();
    config.emit_includes(true);
    for lib in &req.libs {
        config.lib_names(&lib.lib_name, &lib.dll_name);
    }
    let _ = config.find_package(name).map_err(ErrorKind::VcpkgError)?;
    Ok(())
}

/// Probes system libraries via the [`pkg-config`] crate.
fn try_pkg_config(req: &PkgConfigRequirement) -> Result<(), Error> {
    let name = req.name.as_str();
    emit_no_vendor(name);
    let mut config = pkg_config::Config::new();

    if let Some(version_req) = &req.version_req {
        match version_req {
            PkgConfigVersionReq::Range { min, max } => {
                config.range_version(min.as_str()..max.as_str());
            }
            PkgConfigVersionReq::Min { min } => {
                config.range_version(min.as_str()..);
            }
            PkgConfigVersionReq::Max { max } => {
                config.range_version(..=max.as_str());
            }
            PkgConfigVersionReq::Exact { exact } => {
                config.exactly_version(&exact);
            }
        }
    }

    let lib = config.probe(&req.name).map_err(ErrorKind::PkgConfigError)?;
    for include in &lib.include_paths {
        println!("cargo:include={}", include.display());
    }
    Ok(())
}

fn emit_no_vendor(lib_name: &str) {
    let normalized_name = lib_name.to_uppercase().replace("-", "_");
    println!("cargo:rerun-if-env-changed={normalized_name}_NO_VENDOR");
}
