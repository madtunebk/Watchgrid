-- Software motion events from build 87ff6d7 were stored with origin
-- 'watchgrid', which marks events Watchgrid makes itself (outages, manual
-- recordings) and keeps them from being closed at start-up or linked to
-- recordings like other detections.
UPDATE events SET origin = 'software' WHERE source = 'Software motion' AND origin = 'watchgrid';
