-- One live export job per clip and destination (failed ones may repeat).
-- Older duplicates, if any, are marked failed first.
UPDATE export_jobs j SET state = 'failed', message = 'Duplicate of an earlier job'
WHERE state <> 'failed'
  AND EXISTS (
      SELECT 1 FROM export_jobs k
      WHERE k.recording_id = j.recording_id AND k.target_id = j.target_id AND k.state <> 'failed'
        AND (k.created_at, k.id) < (j.created_at, j.id));
CREATE UNIQUE INDEX export_jobs_one_live ON export_jobs (recording_id, target_id) WHERE state <> 'failed';
