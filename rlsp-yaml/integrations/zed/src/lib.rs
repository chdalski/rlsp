use zed_extension_api::{
    self as zed, settings::LspSettings, Architecture, DownloadedFileType, GithubReleaseOptions,
    LanguageServerId, Os, Result, Worktree,
};

const GITHUB_RELEASE_PREFIX: &str = "https://github.com/chdalski/rlsp/releases/download/";

struct RlspYamlExtension;

impl zed::Extension for RlspYamlExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<zed::Command> {
        if let Some(path) = worktree.which("rlsp-yaml") {
            return Ok(zed::Command {
                command: path,
                args: vec![],
                env: vec![],
            });
        }

        let (os, arch) = zed::current_platform();
        let target = platform_target(os, arch)?;

        let release = zed::latest_github_release(
            "chdalski/rlsp",
            GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let version = &release.version;
        let (asset_name, file_type, binary_name) = if matches!(os, Os::Windows) {
            (
                format!("rlsp-yaml-{target}.zip"),
                DownloadedFileType::Zip,
                "rlsp-yaml.exe",
            )
        } else {
            (
                format!("rlsp-yaml-{target}.tar.gz"),
                DownloadedFileType::GzipTar,
                "rlsp-yaml",
            )
        };

        let asset = release
            .assets
            .iter()
            .find(|a| a.name == asset_name)
            .ok_or_else(|| format!("no asset '{asset_name}' in release {version}"))?;

        if !asset.download_url.starts_with(GITHUB_RELEASE_PREFIX) {
            return Err(format!(
                "unexpected download URL for '{asset_name}': {}",
                asset.download_url
            ));
        }

        let install_dir = format!("rlsp-yaml-{version}");
        zed::download_file(&asset.download_url, &install_dir, file_type)
            .map_err(|e| format!("failed to download {asset_name}: {e}"))?;

        let binary_path = format!("{install_dir}/{binary_name}");
        zed::make_file_executable(&binary_path)
            .map_err(|e| format!("failed to make {binary_name} executable: {e}"))?;

        // Remove stale version directories; ignore individual errors (best-effort cleanup).
        if let Ok(entries) = std::fs::read_dir(".") {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with("rlsp-yaml-") && name != install_dir {
                    let _ = std::fs::remove_dir_all(entry.path());
                }
            }
        }

        Ok(zed::Command {
            command: binary_path,
            args: vec![],
            env: vec![],
        })
    }

    fn language_server_initialization_options(
        &mut self,
        server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<serde_json::Value>> {
        Ok(LspSettings::for_worktree(server_id.as_ref(), worktree)
            .ok()
            .and_then(|s| s.initialization_options))
    }

    fn language_server_workspace_configuration(
        &mut self,
        server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<serde_json::Value>> {
        Ok(LspSettings::for_worktree(server_id.as_ref(), worktree)
            .ok()
            .and_then(|s| s.settings))
    }
}

fn platform_target(os: Os, arch: Architecture) -> Result<&'static str> {
    match (os, arch) {
        (Os::Linux, Architecture::X8664) => Ok("x86_64-unknown-linux-gnu"),
        (Os::Linux, Architecture::Aarch64) => Ok("aarch64-unknown-linux-gnu"),
        (Os::Mac, Architecture::X8664) => Ok("x86_64-apple-darwin"),
        (Os::Mac, Architecture::Aarch64) => Ok("aarch64-apple-darwin"),
        (Os::Windows, Architecture::X8664) => Ok("x86_64-pc-windows-msvc"),
        _ => Err(format!(
            "rlsp-yaml: unsupported platform ({os:?}, {arch:?})"
        )),
    }
}

zed::register_extension!(RlspYamlExtension);

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::linux_x86_64(Os::Linux, Architecture::X8664, "x86_64-unknown-linux-gnu")]
    #[case::linux_aarch64(Os::Linux, Architecture::Aarch64, "aarch64-unknown-linux-gnu")]
    #[case::mac_x86_64(Os::Mac, Architecture::X8664, "x86_64-apple-darwin")]
    #[case::mac_aarch64(Os::Mac, Architecture::Aarch64, "aarch64-apple-darwin")]
    #[case::windows_x86_64(Os::Windows, Architecture::X8664, "x86_64-pc-windows-msvc")]
    fn platform_target_supported(
        #[case] os: Os,
        #[case] arch: Architecture,
        #[case] expected: &str,
    ) {
        assert_eq!(platform_target(os, arch).unwrap(), expected);
    }

    #[rstest]
    #[case::windows_aarch64(Os::Windows, Architecture::Aarch64)]
    fn platform_target_unsupported_returns_err(#[case] os: Os, #[case] arch: Architecture) {
        assert!(platform_target(os, arch).is_err());
    }

    #[rstest]
    #[case::windows_aarch64(Os::Windows, Architecture::Aarch64)]
    fn platform_target_err_message_is_descriptive(#[case] os: Os, #[case] arch: Architecture) {
        let err = platform_target(os, arch).unwrap_err();
        assert!(!err.is_empty());
        let lower = err.to_lowercase();
        assert!(
            lower.contains("aarch64") || lower.contains("windows"),
            "error message should name the unsupported platform: {err}"
        );
    }
}
