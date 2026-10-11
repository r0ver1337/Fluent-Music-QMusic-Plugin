use super::*;

impl Provider {
    pub(super) fn get_track_qualities(&mut self, track: &Track) -> Result<ResultData, ApiError> {
        let deadline = Instant::now() + Duration::from_secs(80);
        let song = self.song_info(track, deadline)?;
        media::qualities::list(&song).map(ResultData::TrackQualities)
    }

    pub(super) fn resolve_with_quality(
        &mut self,
        track: Track,
        quality_id: &str,
    ) -> Result<ResultData, ApiError> {
        if quality_id.is_empty() || quality_id.len() > 128 {
            return Err(fail("PARAMETERS", "音质标识无效"));
        }
        let deadline = Instant::now() + Duration::from_secs(80);
        let song = self.song_info(&track, deadline)?;
        self.media
            .source_quality_until(&song, quality_id, deadline)
            .map(ResultData::Media)
    }
}
