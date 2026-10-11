use super::*;

/// Explicit command-line probe using application configuration but NO user session.
/// Only safe status/counts are printed, never raw JSON, IDs, keys, tokens or signed URLs.
pub fn diagnose_recommendations() -> Result<Value, ApiError> {
    let mut client = Client::new(Config::bundled())?;
    let homepage = client
        .homepage_playlists(None, Instant::now() + Duration::from_secs(20))
        .and_then(|v| discovery::opi_feed(&v));
    let result = |r: Result<RecommendationFeed, ApiError>| match r {
        Ok(feed) => {
            json!({"ok":true,"playlist_count":feed.playlists.len(),"track_count":feed.tracks.len()})
        }
        Err(error) => json!({"ok":false,"error":error}),
    };
    Ok(
        json!({"authenticated":false,"path":"openqqmusic_opi_homepage","homepage_get":result(homepage)}),
    )
}
/// Read only this plugin's bound saved session. No refresh, QR, account request,
/// or identity values are emitted; this is not a server authorization check.
pub fn diagnose_saved_session() -> Result<Value, ApiError> {
    let mut provider = Provider::persistent();
    provider.restore_once();
    if let Some(error) = provider.storage_issue {
        return Err(error);
    }
    Ok(saved_session_status(provider.session.as_ref()))
}
/// Explicit authenticated recommendation probe. Uses this plugin's bound restore/
/// refresh lifecycle, not HAR, and emits only profile field names and counts.
pub fn diagnose_recommendation_account() -> Result<Value, ApiError> {
    let mut p = Provider::persistent();
    p.restore_once();
    p.ensure_session()?;
    p.account()?;
    let c = p.client.as_ref().unwrap();
    let s = p.session.as_ref().unwrap();
    let mut recommendation =
        crate::recommendation::RecommendationClient::new(c.config.device_id.clone())?;
    recommendation.set_open_id_account(&c.config.app_id, s)?;
    let fields = recommendation.verify_open_id_account(Instant::now() + Duration::from_secs(30))?;
    let feed = recommendation.feed(Instant::now() + Duration::from_secs(45))?;
    let mut negative = s.clone();
    negative.access = "invalid-token-negative-control".into();
    recommendation.set_open_id_account(&c.config.app_id, &negative)?;
    let rejected = recommendation
        .verify_open_id_account(Instant::now() + Duration::from_secs(15))
        .err();
    Ok(
        json!({"har_read":false,"opi_authorization_verified":true,"flyme_open_id_account_verified":true,
        "invalid_token_control_rejected":rejected.is_some(),"invalid_token_control_error_code":rejected.as_ref().map(|e|e.code.clone()),
        "account_profile_field_names":fields,"playlist_count":feed.playlists.len(),"track_count":feed.tracks.len(),"issues":feed.issues}),
    )
}
pub(super) fn saved_session_status(session: Option<&Session>) -> Value {
    let music_id_state = match session.and_then(|s| s.music_id.as_deref()) {
        None => "absent",
        Some("") => "empty",
        Some("0") => "zero",
        Some(_) => "present",
    };
    json!({"saved_session":session.is_some(),"music_id_state":music_id_state,
        "music_id_available":music_id_state=="present","network_requests_sent":false})
}
