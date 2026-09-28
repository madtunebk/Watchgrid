-- Armed surveillance (Dashboard): one row per armed camera, with the
-- settings arming changed (motion on/off, detection source, recording
-- mode) as they were before, to put back on disarm.
CREATE TABLE armed_cameras (
    camera_id text PRIMARY KEY REFERENCES cameras(id) ON DELETE CASCADE,
    armed_at timestamptz NOT NULL DEFAULT now(),
    before jsonb NOT NULL
);
