use fluent_plugin_api::{ApiError, LoginState, ResultData, Track};
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginManifest {
    #[serde(default)]
    pub Network: Vec<String>,
}
#[derive(Clone)]
pub struct NetworkPolicy {
    Sites: Vec<Site>,
}
#[derive(Clone)]
struct Site {
    Scheme: String,
    Host: String,
    Port: u16,
    Subdomains: bool,
}
pub fn failed() -> ApiError {
    ApiError::new("NETWORK", "网络请求失败：目标未声明或请求被拒绝")
}
impl NetworkPolicy {
    pub fn new(manifest: &PluginManifest) -> Result<Self, ApiError> {
        if manifest.Network.len() > 128 {
            return Err(failed());
        }
        let mut sites = Vec::new();
        for entry in &manifest.Network {
            if entry.len() > 2048 || entry.trim() != entry {
                return Err(failed());
            }
            let (scheme, rest) = entry.split_once("://").ok_or_else(failed)?;
            let subdomains = rest.starts_with("*.");
            let normalized = format!("{scheme}://{}", rest.strip_prefix("*.").unwrap_or(rest));
            let url = url::Url::parse(&normalized).map_err(|_| failed())?;
            let host = url.host_str().ok_or_else(failed)?;
            if !matches!(url.scheme(), "http" | "https")
                || !url.username().is_empty()
                || url.password().is_some()
                || url.path() != "/"
                || url.query().is_some()
                || url.fragment().is_some()
                || normalized.contains('*')
                || host.ends_with('.')
                || host.is_empty()
                || entry.contains(['\\', '\r', '\n', '\t'])
                || (subdomains
                    && (!host.contains('.')
                        || host.parse::<std::net::IpAddr>().is_ok()
                        || host.starts_with('[')))
            {
                return Err(failed());
            }
            sites.push(Site {
                Scheme: url.scheme().into(),
                Host: host.into(),
                Port: url.port_or_known_default().ok_or_else(failed)?,
                Subdomains: subdomains,
            });
        }
        Ok(Self { Sites: sites })
    }
    pub fn check(&self, raw: &str) -> Result<url::Url, ApiError> {
        if raw.len() > 16384 || raw.trim() != raw || raw.contains(['\\', '\r', '\n', '\t']) {
            return Err(failed());
        }
        let url = url::Url::parse(raw).map_err(|_| failed())?;
        if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
            return Err(failed());
        }
        let host = url.host_str().ok_or_else(failed)?;
        if self.Sites.iter().any(|site| {
            site.Scheme == url.scheme()
                && Some(site.Port) == url.port_or_known_default()
                && if site.Subdomains {
                    host.strip_suffix(&site.Host)
                        .is_some_and(|prefix| prefix.ends_with('.') && prefix.len() > 1)
                } else {
                    site.Host == host
                }
        }) {
            Ok(url)
        } else {
            Err(failed())
        }
    }
    pub fn checkResult(&self, result: &ResultData) -> Result<(), ApiError> {
        let image = |url: &Option<String>| -> Result<(), ApiError> {
            if let Some(url) = url {
                self.check(url)?;
            }
            Ok(())
        };
        let track = |track: &Track| -> Result<(), ApiError> {
            image(&track.cover_url)?;
            image(&track.thumbnail_url)
        };
        match result {
            ResultData::Account(account)
            | ResultData::LoginState(LoginState::Authorized(account)) => {
                image(&account.avatar_url)?
            }
            ResultData::Tracks(page) => {
                for item in &page.items {
                    track(item)?;
                }
            }
            ResultData::Playlists(items) => {
                for item in items {
                    image(&item.cover_url)?;
                }
            }
            ResultData::RecommendationFeed(feed) => {
                for item in &feed.tracks {
                    track(item)?;
                }
                for item in &feed.playlists {
                    image(&item.cover_url)?;
                }
            }
            ResultData::Media(fluent_plugin_api::MediaSource::Http { url, .. }) => {
                self.check(url)?;
            }
            _ => {}
        }
        Ok(())
    }
}
#[cfg(test)]
#[path = "networkPolicyTests.rs"]
mod tests;
