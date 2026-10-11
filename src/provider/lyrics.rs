use super::*;

impl Provider {
    pub(super) fn get_lyrics(&mut self, track: &Track) -> Result<ResultData, ApiError> {
        let parameters = crate::lyrics::request_parameters(track)?;
        let deadline = Instant::now() + Duration::from_secs(35);
        if self.client.is_none() {
            self.client = Some(Client::new(Config::bundled())?);
        }
        if self.session.is_some() {
            self.ensure_session_until(deadline)?;
        }
        let data = self.request_until(crate::lyrics::COMMAND, &parameters, deadline)?;
        Ok(ResultData::Lyrics(crate::lyrics::parse_response(
            data,
            track.duration_ms,
        )?))
    }
}
