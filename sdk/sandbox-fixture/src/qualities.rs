use fluent_plugin_api::*;

pub fn list(hasTrack: bool) -> QualityList {
    QualityList {
        Levels: vec![
            QualityLevel {
                Priority: 1,
                Name: "无损".into(),
                Disabled: false,
            },
            QualityLevel {
                Priority: 2,
                Name: "标准".into(),
                Disabled: false,
            },
        ],
        Items: if hasTrack {
            vec![
                TrackQuality {
                    Id: "lossless".into(),
                    Name: "无损".into(),
                    Priority: 1,
                    FileSizeBytes: Some(32_000_000),
                    Playable: false,
                    UnavailableReason: Some("当前账号未获授权".into()),
                },
                TrackQuality {
                    Id: "standard".into(),
                    Name: "标准".into(),
                    Priority: 2,
                    FileSizeBytes: None,
                    Playable: true,
                    UnavailableReason: None,
                },
            ]
        } else {
            vec![]
        },
    }
}
