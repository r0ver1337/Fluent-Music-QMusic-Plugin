mod artwork;
#[cfg(not(test))]
use fluent_plugin_abi::network::http;
#[cfg(test)]
use reqwest::blocking as http;
mod broker_media;
pub mod compatibility;
mod discovery;
mod lyrics;
mod media;
mod plugin_config;
mod protocol;
mod provider;
mod qmc;
pub mod qrc;
mod recommendation;
mod session_store;

fn main() {
    let arguments = std::env::args().collect::<Vec<_>>();
    if arguments
        .get(1)
        .is_some_and(|argument| argument == "--replay-lyrics")
    {
        let result = if arguments.len() == 3 {
            provider::replay_lyrics(std::path::Path::new(&arguments[2]))
        } else {
            Err(fluent_plugin_api::ApiError::new(
                "PARAMETERS",
                "需要歌词响应样本路径",
            ))
        };
        match result {
            Ok(value) => println!("{value}"),
            Err(error) => {
                println!("{}", serde_json::json!({"error":error}));
                std::process::exit(1);
            }
        }
        return;
    }
    if arguments
        .get(1)
        .is_some_and(|argument| argument == "--capture-lyrics")
    {
        let result = if arguments.len() == 5 {
            provider::capture_lyrics(
                &arguments[2],
                &arguments[3],
                std::path::Path::new(&arguments[4]),
            )
        } else {
            Err(fluent_plugin_api::ApiError::new(
                "PARAMETERS",
                "需要歌曲名、歌手和新样本目录",
            ))
        };
        match result {
            Ok(value) => println!("{value}"),
            Err(error) => {
                println!("{}", serde_json::json!({"error":error}));
                std::process::exit(1);
            }
        }
        return;
    }
    if arguments.iter().any(|arg| {
        compatibility::matches_flag(compatibility::RecommendationCommand::AccountDiagnostic, arg)
    }) {
        let result = match provider::diagnose_recommendation_account() {
            Ok(value) => value,
            Err(error) => serde_json::json!({"har_read":false,"error":error}),
        };
        println!("{result}");
        return;
    }
    if arguments.iter().any(|arg| {
        compatibility::matches_flag(compatibility::RecommendationCommand::GuestDiagnostic, arg)
    }) {
        let result = match recommendation::RecommendationClient::new(
            uuid::Uuid::new_v4().simple().to_string(),
        )
        .and_then(|mut c| c.feed(std::time::Instant::now() + std::time::Duration::from_secs(50)))
        {
            Ok(feed) => serde_json::json!({"path":"flyme_guest_homepage","har_read":false,
                "account_credentials_used":false,"playlist_count":feed.playlists.len(),"track_count":feed.tracks.len(),"issues":feed.issues}),
            Err(error) => {
                serde_json::json!({"path":"flyme_guest_homepage","har_read":false,"error":error})
            }
        };
        println!("{result}");
        return;
    }
    if std::env::args().any(|arg| arg == "--diagnose-session") {
        let result = match provider::diagnose_saved_session() {
            Ok(value) => value,
            Err(error) => serde_json::json!({"network_requests_sent":false,"error":error}),
        };
        println!("{result}");
        return;
    }
    if std::env::args().any(|arg| arg == "--diagnose-recommendations") {
        let result = match provider::diagnose_recommendations() {
            Ok(value) => value,
            Err(error) => serde_json::json!({"authenticated":false,"error":error}),
        };
        println!("{result}");
        return;
    }
    let mut provider = provider::Provider::persistent();
    if fluent_plugin_api::serve(|operation| provider.handle(operation)).is_err() {
        // Protocol stdout must never contain diagnostics or raw network data.
        std::process::exit(1);
    }
}
