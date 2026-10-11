//! Legacy command/configuration spellings are accepted only at input boundaries.

#[derive(Clone, Copy)]
pub enum RecommendationCommand {
    Homepage,
    GuestDiagnostic,
    AccountDiagnostic,
}

pub fn matches_flag(command: RecommendationCommand, argument: &str) -> bool {
    match command {
        RecommendationCommand::Homepage => {
            matches!(argument, "--signed-homepage" | "--flyme-homepage")
        }
        RecommendationCommand::GuestDiagnostic => matches!(
            argument,
            "--diagnose-recommendation-guest" | "--diagnose-flyme"
        ),
        RecommendationCommand::AccountDiagnostic => matches!(
            argument,
            "--diagnose-recommendation-account" | "--diagnose-flyme-account"
        ),
    }
}

pub fn uses_signed_homepage(name: &str, value: &str) -> bool {
    matches!(
        name,
        "homepageMode" | "homepage_mode" | "recommendation_source"
    ) && matches!(value, "flyme" | "signed")
}

// Keep the previously published Rust diagnostic names as aliases, not internal implementations.
pub mod legacy_api {
    pub use crate::provider::{
        capture_lyrics as captureLyrics, diagnose_recommendation_account as diagnoseFlymeAccount,
        diagnose_recommendations as diagnoseRecommendations,
        diagnose_saved_session as diagnoseSavedSession, replay_lyrics as replayLyrics,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_and_role_based_flags_select_the_same_commands() {
        for (command, old, new) in [
            (
                RecommendationCommand::Homepage,
                "--flyme-homepage",
                "--signed-homepage",
            ),
            (
                RecommendationCommand::GuestDiagnostic,
                "--diagnose-flyme",
                "--diagnose-recommendation-guest",
            ),
            (
                RecommendationCommand::AccountDiagnostic,
                "--diagnose-flyme-account",
                "--diagnose-recommendation-account",
            ),
        ] {
            assert!(matches_flag(command, old));
            assert!(matches_flag(command, new));
            assert!(!matches_flag(command, "--unrelated"));
        }
    }

    #[test]
    fn old_configuration_value_and_new_source_name_remain_compatible() {
        for name in ["homepageMode", "homepage_mode", "recommendation_source"] {
            assert!(uses_signed_homepage(name, "flyme"));
            assert!(uses_signed_homepage(name, "signed"));
            assert!(!uses_signed_homepage(name, "unknown"));
        }
        assert!(!uses_signed_homepage("unrelated", "flyme"));
    }
}
