ALTER TABLE pod0_playback_state ADD COLUMN active_segment_start_ms INTEGER;
ALTER TABLE pod0_playback_state ADD COLUMN active_segment_end_ms INTEGER;
ALTER TABLE pod0_playback_state ADD COLUMN active_segment_label TEXT;
ALTER TABLE pod0_playback_state ADD COLUMN last_position_committed_at_ms INTEGER;
