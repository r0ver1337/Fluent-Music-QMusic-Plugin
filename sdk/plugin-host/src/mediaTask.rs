#![allow(non_snake_case)]
use crate::{MediaFile, fileBroker::MediaArtifact};
use fluent_plugin_api::ApiError;
use std::{collections::BTreeMap, sync::Arc};

pub enum MediaTask {
    Ready(MediaFile),
    Download {
        Output: MediaArtifact,
        Url: String,
        Headers: BTreeMap<String, String>,
    },
}

impl MediaTask {
    pub fn finish(
        self,
        wanted: impl Fn() -> bool,
        speculative: bool,
    ) -> Result<MediaFile, ApiError> {
        if !wanted() {
            return Err(ApiError::new("CANCELLED", "音频缓存已取消"));
        }
        match self {
            Self::Ready(file) => Ok(file),
            Self::Download {
                mut Output,
                Url,
                Headers,
            } => {
                crate::transport::downloadControlled(
                    Url,
                    Headers,
                    &mut Output.File,
                    wanted,
                    speculative,
                )?;
                Ok(MediaFile {
                    path: Output.Path.clone(),
                    _temporary: None,
                    _mediaLease: Some(Arc::new(Output)),
                })
            }
        }
    }
}
