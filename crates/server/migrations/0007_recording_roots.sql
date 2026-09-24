-- The recordings folder can change at runtime; each recording remembers
-- the root it was written under. NULL = the configured default folder.
ALTER TABLE recordings ADD COLUMN root TEXT;
