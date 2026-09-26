-- Failed uploads are tried again later: how many tries so far, and when the
-- next one is due (NULL: queued for now).
ALTER TABLE export_jobs ADD COLUMN attempts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE export_jobs ADD COLUMN retry_at TIMESTAMPTZ;
CREATE INDEX export_jobs_retry_due ON export_jobs (retry_at) WHERE retry_at IS NOT NULL;
