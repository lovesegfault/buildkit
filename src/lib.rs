// WINDOWS SUPPORT OMG

use camino::Utf8PathBuf;
use cargo_metadata::MetadataCommand;
use serde::Deserialize;

// This will be the builder pattern thing that people interact with in their build.rs
struct BuildKit {
    metadata: BuildKitMetadata,
}

impl BuildKit {
    pub fn from_metadata() -> Result<Self, ConfigurationError> {
        let manifest_dir = match std::env::var("CARGO_MANIFEST_DIR") {
            Ok(path) => Utf8PathBuf::from(path),
            Err(e) => return Err(ConfigurationError::NoCargoManifestDirInEnv(e)),
        };
        let manifest_path = manifest_dir.join("Cargo.toml");
        let metadata = MetadataCommand::new()
            .manifest_path(&manifest_path)
            .exec()
            .map_err(ConfigurationError::CargoMetadataError)?;

        let root_package = {
            let root_id = metadata
                .resolve
                .and_then(|r| r.root)
                .ok_or_else(|| ConfigurationError::InvalidCargoMetadata("resolve.root".into()))?;
            metadata
                .packages
                .into_iter()
                .find(|pkg| pkg.id == root_id)
                .ok_or_else(|| {
                    ConfigurationError::InvalidCargoMetadata(format!(
                        r#"packages.id = "{root_id}""#
                    ))
                })?
        };
        let metadata: BuildKitMetadata = serde_json::from_value(root_package.metadata)?;

        Ok(BuildKit { metadata })
    }
}

#[derive(Debug, thiserror::Error)]
enum ConfigurationError {
    #[error("Failed to parse cargo metadata")]
    CargoMetadataError(#[from] cargo_metadata::Error),
    #[error("Did not find $CARGO_MANIFEST_DIR in env")]
    NoCargoManifestDirInEnv(#[source] std::env::VarError),
    #[error("Failed to find `{0}` in cargo metadata output")]
    InvalidCargoMetadata(String),
    #[error("Failed to deserialize `package.metadata.buildkit`")]
    Json(#[from] serde_json::Error),
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

#[derive(Deserialize)]
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

// |ctx: VendoredBuildContext| -> Result<(), BuildFailure>
// struct VendoredBuildContext {
//     source_path: Utf8PathBuf,
// }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {}
}
