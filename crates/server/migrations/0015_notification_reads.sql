-- Read / unread per user: one row per notification a user has read.
CREATE TABLE notification_reads (
    notification_id TEXT NOT NULL REFERENCES notifications (id) ON DELETE CASCADE,
    user_id         BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    PRIMARY KEY (notification_id, user_id)
);
CREATE INDEX notification_reads_user ON notification_reads (user_id);
-- What was read until now was read for everyone: keep it that way.
INSERT INTO notification_reads (notification_id, user_id)
    SELECT n.id, u.id FROM notifications n CROSS JOIN users u WHERE n.read;
ALTER TABLE notifications DROP COLUMN read;
