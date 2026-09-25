-- Recordings can be exported directly (continuous clips have no event).
ALTER TABLE export_jobs ALTER COLUMN event_id DROP NOT NULL;
