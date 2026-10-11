#![allow(non_snake_case, non_upper_case_globals)]
use crate::ApiError;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const maxQualityEntries: usize = 64;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QualityLevel {
    pub Priority: u32,
    pub Name: String,
    #[serde(default)]
    pub Disabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrackQuality {
    pub Id: String,
    pub Name: String,
    pub Priority: u32,
    pub FileSizeBytes: Option<u64>,
    pub Playable: bool,
    pub UnavailableReason: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QualityList {
    pub Levels: Vec<QualityLevel>,
    pub Items: Vec<TrackQuality>,
}

impl QualityList {
    pub fn validate(&self) -> Result<(), ApiError> {
        let invalid = || ApiError::new("QUALITY_SCHEMA", "插件音质列表无效");
        if self.Levels.is_empty()
            || self.Levels.len() > maxQualityEntries
            || self.Items.len() > maxQualityEntries
        {
            return Err(invalid());
        }
        let mut priorities = HashSet::new();
        for level in &self.Levels {
            if level.Priority == 0
                || level.Priority > i32::MAX as u32
                || level.Name.trim().is_empty()
                || level.Name.len() > 256
                || !priorities.insert(level.Priority)
            {
                return Err(invalid());
            }
        }
        let mut ids = HashSet::new();
        for item in &self.Items {
            if item.Id.is_empty()
                || item.Id.len() > 128
                || item.Name.trim().is_empty()
                || item.Name.len() > 256
                || !priorities.contains(&item.Priority)
                || !ids.insert(&item.Id)
                || item.FileSizeBytes == Some(0)
                || item
                    .UnavailableReason
                    .as_ref()
                    .is_some_and(|value| value.len() > 512)
                || (item.Playable && item.UnavailableReason.is_some())
                || (item.Playable
                    && self
                        .Levels
                        .iter()
                        .any(|level| level.Priority == item.Priority && level.Disabled))
            {
                return Err(invalid());
            }
        }
        Ok(())
    }

    pub fn candidates(&self, priority: u32) -> Vec<&TrackQuality> {
        let mut items: Vec<_> = self
            .Items
            .iter()
            .filter(|item| item.Playable && item.Priority >= priority)
            .collect();
        items.sort_by(|left, right| (left.Priority, &left.Id).cmp(&(right.Priority, &right.Id)));
        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list() -> QualityList {
        QualityList {
            Levels: vec![
                QualityLevel {
                    Priority: 1,
                    Name: "无损".into(),
                    Disabled: false,
                },
                QualityLevel {
                    Priority: 3,
                    Name: "标准".into(),
                    Disabled: false,
                },
            ],
            Items: vec![
                TrackQuality {
                    Id: "sq".into(),
                    Name: "无损".into(),
                    Priority: 1,
                    FileSizeBytes: Some(32_000_000),
                    Playable: false,
                    UnavailableReason: Some("当前账号未获授权".into()),
                },
                TrackQuality {
                    Id: "standard".into(),
                    Name: "标准".into(),
                    Priority: 3,
                    FileSizeBytes: None,
                    Playable: true,
                    UnavailableReason: None,
                },
            ],
        }
    }

    #[test]
    fn priorityOnlyFallsTowardLowerPreference() {
        let mut value = list();
        assert!(value.validate().is_ok());
        assert_eq!(value.candidates(1)[0].Id, "standard");
        value.Items[0].Playable = true;
        value.Items[0].UnavailableReason = None;
        assert_eq!(value.candidates(2)[0].Id, "standard");
        assert!(value.candidates(4).is_empty());
    }

    #[test]
    fn malformedAndDuplicateEntriesAreRejected() {
        let mut value = list();
        value.Items.push(value.Items[0].clone());
        assert!(value.validate().is_err());
        value.Items.pop();
        value.Items[0].Priority = 2;
        assert!(value.validate().is_err());
        value.Items[0].Priority = 1;
        value.Items[0].FileSizeBytes = Some(0);
        assert!(value.validate().is_err());
    }

    #[test]
    fn unknownSizeAndPermissionStateSurviveCamelCaseWire() {
        let value = list();
        let json = serde_json::to_value(&value).unwrap();
        assert!(json["items"][1]["fileSizeBytes"].is_null());
        assert_eq!(json["items"][0]["playable"], false);
        assert_eq!(serde_json::from_value::<QualityList>(json).unwrap(), value);
    }

    #[test]
    fn missingDisabledFlagPreservesOldRecordsButDisabledLevelsCannotBePlayable() {
        let level: QualityLevel = serde_json::from_str(r#"{"priority":1,"name":"无损"}"#).unwrap();
        assert!(!level.Disabled);
        let mut value = list();
        value.Levels[0].Disabled = true;
        assert!(value.validate().is_ok());
        value.Items[0].Playable = true;
        value.Items[0].UnavailableReason = None;
        assert!(value.validate().is_err());
    }
}
