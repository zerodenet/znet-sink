use super::*;
pub(crate) fn validate_registration(registration: &Registration) -> Result<()> {
    if registration
        .product_id
        .as_ref()
        .is_some_and(|id| !valid_id(id))
    {
        return Err("invalid marketplace product identity".into());
    }
    Ok(())
}

pub(crate) fn validate_release(
    registration: &Registration,
    release: &MarketplaceRelease,
) -> Result<()> {
    registration.validate()?;
    let repository_path = registration.repository_path()?;
    semver::Version::parse(&release.version)?;
    if !matches!(release.channel.as_str(), "stable" | "rc" | "dev")
        || release.published_at.is_empty()
        || !release
            .surfaces
            .iter()
            .all(|value| registration.surfaces.contains(value))
        || !release
            .capabilities
            .iter()
            .all(|value| registration.capabilities.contains(value))
        || release.artifacts.is_empty()
    {
        return Err("invalid marketplace release boundary".into());
    }
    let notes = reqwest::Url::parse(&release.notes_url)?;
    if !https(&notes) {
        return Err("invalid marketplace release notes".into());
    }
    let prefix = format!("/{repository_path}/releases/download/v{}/", release.version);
    let mut platforms = BTreeSet::new();
    for artifact in &release.artifacts {
        if let Some(signature) = &artifact.signature {
            use base64::Engine;
            if signature.algorithm != "ed25519"
                || base64::engine::general_purpose::STANDARD
                    .decode(&signature.value)?
                    .len()
                    != 64
            {
                return Err("invalid marketplace signature".into());
            }
        }
        let artifact_url = reqwest::Url::parse(&artifact.url)?;
        if !matches!(
            artifact.os.as_str(),
            "any" | "linux" | "darwin" | "windows" | "android" | "ios"
        ) || !matches!(artifact.arch.as_str(), "any" | "amd64" | "arm64")
            || (artifact.os != "any" && artifact.arch == "any")
            || !platforms.insert((&artifact.os, &artifact.arch))
            || artifact.size == 0
            || artifact.size > 128 * 1024 * 1024
            || artifact.sha256.len() != 64
            || !artifact
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || !https(&artifact_url)
            || artifact_url.query().is_some()
            || artifact_url.fragment().is_some()
            || artifact_url.host_str() != Some("github.com")
            || !artifact_url.path().starts_with(&prefix)
            || artifact_url.path()[prefix.len()..].contains('/')
            || !artifact_url.path().ends_with(".zspkg")
        {
            return Err("invalid marketplace artifact".into());
        }
    }
    Ok(())
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && (value.as_bytes()[0].is_ascii_lowercase() || value.as_bytes()[0].is_ascii_digit())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
}

pub(crate) fn https(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
}
