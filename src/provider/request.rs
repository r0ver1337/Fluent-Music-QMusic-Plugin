use super::*;

impl Provider {
    pub(super) fn request(
        &mut self,
        command: &str,
        values: &[(&str, String)],
    ) -> Result<Value, ApiError> {
        success(self.client.as_mut().unwrap().request(
            command,
            values,
            self.session.as_ref(),
            &[],
            false,
        )?)
    }
    pub(super) fn request_until(
        &mut self,
        command: &str,
        values: &[(&str, String)],
        deadline: Instant,
    ) -> Result<Value, ApiError> {
        success(self.client.as_mut().unwrap().request_until(
            command,
            values,
            self.session.as_ref(),
            &[],
            false,
            deadline,
        )?)
    }
}
