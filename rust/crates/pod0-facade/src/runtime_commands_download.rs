impl FacadeState {
    fn accept_download_command(
        &mut self,
        envelope: &CommandEnvelope,
        fingerprint: &str,
        command: ApplicationCommand,
    ) {
        match command {
            ApplicationCommand::RequestEpisodeDownload { episode_id, origin } => {
                self.request_episode_download(envelope, fingerprint, episode_id, origin)
            }
            ApplicationCommand::ReportAutomaticDownloadCandidates {
                podcast_id,
                episode_ids,
            } => self.report_automatic_download_candidates(
                envelope,
                fingerprint,
                podcast_id,
                episode_ids,
            ),
            ApplicationCommand::CancelEpisodeDownload {
                episode_id,
                expected_workflow_revision,
            } => self.cancel_episode_download(
                envelope,
                fingerprint,
                episode_id,
                expected_workflow_revision,
            ),
            ApplicationCommand::RemoveEpisodeDownload {
                episode_id,
                expected_workflow_revision,
            } => self.remove_episode_download(
                envelope,
                fingerprint,
                episode_id,
                expected_workflow_revision,
            ),
            ApplicationCommand::ObserveDownloadEnvironment { observation } => {
                self.observe_download_environment(envelope, fingerprint, observation)
            }
            _ => unreachable!("download command dispatch"),
        }
    }
}
