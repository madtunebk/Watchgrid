-- Events of a recording: looked up to work out a clip's protection, to
-- unlink events when the clip is deleted, and for the clip's event list.
CREATE INDEX IF NOT EXISTS events_recording_id ON events (recording_id) WHERE recording_id IS NOT NULL;
