// WINDOWS SUPPORT OMG

use camino::Utf8PathBuf;
use cargo_metadata::MetadataCommand;
use serde::Deserialize;

// This will be the builder pattern thing that people interact with in their build.rs
pub struct BuildKit {
    metadata: BuildKitMetadata,
}

impl BuildKit {
    pub fn from_metadata() -> Result<Self, Error> {
        let manifest_dir = match std::env::var("CARGO_MANIFEST_DIR") {
            Ok(path) => Utf8PathBuf::from(path),
            Err(e) => return Err(ErrorKind::NoCargoManifestDirInEnv(e).into()),
        };
        let manifest_path = manifest_dir.join("Cargo.toml");
        let metadata = MetadataCommand::new()
            .manifest_path(&manifest_path)
            .exec()
            .map_err(ErrorKind::CargoMetadataError)?;

        let root_package = {
            let root_id = metadata
                .resolve
                .and_then(|r| r.root)
                .ok_or_else(|| ErrorKind::InvalidCargoMetadata("resolve.root".into()))?;
            metadata
                .packages
                .into_iter()
                .find(|pkg| pkg.id == root_id)
                .ok_or_else(|| {
                    ErrorKind::InvalidCargoMetadata(format!(r#"packages.id = "{root_id}""#))
                })?
        };
        let metadata = serde_json::from_value(root_package.metadata).map_err(ErrorKind::Json)?;

        Ok(BuildKit { metadata })
    }

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
            BuildKitMode::VcPkg => {
                let req = self
                    .metadata
                    .vcpkg
                    .as_ref()
                    .ok_or_else(|| ErrorKind::NoVcpkgRequirementSpecified)?;
                try_vcpkg(req)
            }
        }
    }

    // TODO: ways for external build systems to override
    fn mode(&self) -> Result<BuildKitMode, Error> {
        if matches!(self.metadata.default_mode, BuildKitMode::VendoredBuild) {
            return Ok(BuildKitMode::VendoredBuild);
        }
        let target = std::env::var("TARGET").map_err(|e| ErrorKind::NoTargetInEnv(e))?;
        // TODO: should we relax it to `-windows-`?
        // Some people seems to use vcpkg with mingw: https://www.reddit.com/r/cpp/comments/p1655e/comment/h8bly7v
        //
        // TODO: should we retry if vcpkg found nothing?
        // curl-sys falls back to pkg_config when vcpkg failed.
        // https://github.com/alexcrichton/curl-rust/blob/c01261310f13c85dc70d4e8a1ef87504662a1154/curl-sys/build.rs#L30-L37
        if target.ends_with("-windows-msvc") {
            Ok(BuildKitMode::VcPkg)
        } else {
            Ok(BuildKitMode::PkgConfig)
        }
    }
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
    #[error("Failed to parse cargo metadata")]
    CargoMetadataError(#[from] cargo_metadata::Error),
    #[error("Did not find $CARGO_MANIFEST_DIR in env")]
    NoCargoManifestDirInEnv(#[source] std::env::VarError),
    #[error("Failed to find `{0}` in cargo metadata output")]
    InvalidCargoMetadata(String),
    #[error("Failed to deserialize `package.metadata.buildkit`")]
    Json(#[from] serde_json::Error),
    #[error("vendored mode is set but no vendored source specified")]
    NoVendoredSourceSpecified,
    #[error("pkg-config mode is set but no pkg-config requirement specified")]
    NoPkgConfigRequirementSpecified,
    #[error("vcpkg mode is set but no vcpkg requirement specified")]
    NoVcpkgRequirementSpecified,
    #[error("Did not find $TARGET in env")]
    NoTargetInEnv(#[source] std::env::VarError),
    #[error("vcpkg failed to probe")]
    VcpkgError(#[from] vcpkg::Error),
    #[error("pkg-config failed to probe")]
    PkgConfigError(#[from] pkg_config::Error),
    #[error(transparent)]
    Custom(Box<dyn std::error::Error>),
}

// This will represent the data that folks can specify within their Cargo.toml
// libgit2: name + version range for pkg-config
#[derive(Deserialize)]
struct BuildKitMetadata {
    pkg_config: Option<PkgConfigRequirement>,
    vcpkg: Option<VcPkgRequirement>,
    vendored_source: Option<VendoredSource>,
    default_mode: BuildKitMode,
}

#[derive(Deserialize, Clone, Copy)]
enum BuildKitMode {
    PkgConfig,
    VcPkg,
    VendoredBuild,
}

// TODO: Rename
#[derive(Deserialize)]
struct PkgConfigRequirement {
    name: String,
    version_req: Option<PkgConfigVersionReq>,
}

#[derive(Deserialize)]
enum PkgConfigVersionReq {
    Range { min: String, max: String },
    Min(String),
    Max(String),
    Exact(String),
}

#[derive(Deserialize)]
struct VcPkgRequirement {
    name: String,
    libs: Vec<VcPkgLibName>,
}

#[derive(Deserialize)]
struct VcPkgLibName {
    lib_name: String,
    dll_name: String,
}

#[derive(Deserialize)]
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

    /// Gets the path to the downloaded source.
    pub fn source_path(&self) -> &Utf8PathBuf {
        &self.source_path
    }
}

fn try_vcpkg(req: &VcPkgRequirement) -> Result<(), Error> {
    todo!()
}

fn try_pkg_config(req: &PkgConfigRequirement) -> Result<(), Error> {
    todo!()
}
