//! Recent server log lines kept in memory for the System page (a bounded
//! ring buffer fed by a `tracing` layer). Nothing is written to the DB.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use watchgrid_model::{LogEntry, LogLevel, LogQuery};

const CAPACITY: usize = 2000;

#[derive(Clone, Default)]
pub struct LogBuffer(Arc<Mutex<Ring>>);

#[derive(Default)]
struct Ring {
    next_id: u64,
    lines: VecDeque<LogEntry>,
}

impl LogBuffer {
    fn push(&self, level: LogLevel, source: String, message: String) {
        let mut ring = self.0.lock().expect("log buffer lock");
        ring.next_id += 1;
        let id = format!("log-{}", ring.next_id);
        if ring.lines.len() == CAPACITY {
            ring.lines.pop_front();
        }
        ring.lines.push_back(LogEntry { id, time: Utc::now(), level, source, message });
    }

    /// The newest matching lines, oldest first.
    pub fn query(&self, q: &LogQuery) -> Vec<LogEntry> {
        let needle = q.search.as_deref().unwrap_or("").to_lowercase();
        let ring = self.0.lock().expect("log buffer lock");
        let mut out: Vec<LogEntry> = ring
            .lines
            .iter()
            .rev()
            .filter(|l| q.min_level.is_none_or(|m| l.level >= m))
            .filter(|l| needle.is_empty() || l.message.to_lowercase().contains(&needle) || l.source.to_lowercase().contains(&needle))
            .take(q.limit.unwrap_or(500) as usize)
            .cloned()
            .collect();
        out.reverse();
        out
    }
}

/// `tracing` layer that copies every event into the buffer.
pub struct CaptureLayer(pub LogBuffer);

impl<S: Subscriber> Layer<S> for CaptureLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        let level = match *meta.level() {
            Level::ERROR => LogLevel::Error,
            Level::WARN => LogLevel::Warn,
            Level::INFO => LogLevel::Info,
            _ => LogLevel::Debug,
        };
        let source = meta.target().strip_prefix("watchgrid::").unwrap_or(meta.target()).to_string();
        let mut text = Text::default();
        event.record(&mut text);
        self.0.push(level, source, text.finish());
    }
}

/// "message key=value …", like the console output.
#[derive(Default)]
struct Text {
    message: String,
    fields: String,
}

impl Text {
    fn finish(self) -> String {
        if self.fields.is_empty() { self.message } else { format!("{}{}", self.message, self.fields) }
    }
}

impl Visit for Text {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.message, "{value:?}");
        } else {
            let _ = write!(self.fields, " {}={value:?}", field.name());
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message.push_str(value);
        } else {
            let _ = write!(self.fields, " {}={value}", field.name());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_newest_lines_and_filters() {
        let buf = LogBuffer::default();
        for i in 0..CAPACITY + 5 {
            buf.push(if i % 2 == 0 { LogLevel::Info } else { LogLevel::Warn }, "recorder".into(), format!("line {i}"));
        }
        let all = buf.query(&LogQuery { limit: Some(u32::MAX), ..Default::default() });
        assert_eq!(all.len(), CAPACITY);
        assert_eq!(all.last().unwrap().message, format!("line {}", CAPACITY + 4), "oldest first, newest last");

        let warn = buf.query(&LogQuery { min_level: Some(LogLevel::Warn), limit: Some(3), ..Default::default() });
        assert!(warn.iter().all(|l| l.level == LogLevel::Warn) && warn.len() == 3);
        let found = buf.query(&LogQuery { search: Some("LINE 7".into()), ..Default::default() });
        assert!(found.iter().all(|l| l.message.contains("line 7")));
    }
}
