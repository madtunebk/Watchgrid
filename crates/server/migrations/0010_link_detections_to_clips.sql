-- Detections without a clip whose start falls inside a saved recording of
-- the same camera get that recording (software motion events of build
-- 87ff6d7, and detections during a continuous clip that began before a
-- restart). New events are linked the same way when a recording ends.
UPDATE events e SET recording_id = r.id
FROM recordings r
WHERE e.recording_id IS NULL
  AND e.origin <> 'watchgrid'
  AND r.camera_id = e.camera_id
  AND e.start_time >= r.start_time
  AND e.start_time <= r.end_time;
