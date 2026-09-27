//! One `<video>` element fed by Media Source Extensions from the live
//! WebSocket. Reconnects by itself; keeps playback close to live.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::{Rc, Weak};

use gloo_timers::callback::Timeout;
use js_sys::ArrayBuffer;
use leptos::prelude::*;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen::closure::Closure;
use web_sys::{BinaryType, Event, HtmlVideoElement, MediaSource, MessageEvent, SourceBuffer, SourceBufferAppendMode, Url, WebSocket};

/// Seconds behind the newest frame before playback jumps forward.
const MAX_LATENCY: f64 = 1.5;
/// Where to land after a jump, relative to the newest frame.
const CATCH_UP_TO: f64 = 0.3;
/// Buffered history kept behind the playhead, in seconds.
const KEEP_BEHIND: f64 = 10.0;
const TRIM_AFTER: f64 = 30.0;
const RECONNECT_MS: u32 = 2_000;

#[derive(Debug, Clone, PartialEq)]
pub enum PlayerState {
    Connecting,
    Playing,
    /// The server can't reach the camera right now (it keeps retrying).
    Offline(String),
    /// This browser can't play the stream.
    Unsupported(String),
}

#[derive(serde::Deserialize)]
struct Control {
    #[serde(rename = "type")]
    kind: String,
    codec: Option<String>,
    /// The stream carries audio (init message).
    #[serde(default)]
    audio: bool,
    state: Option<String>,
    reason: Option<String>,
}

pub struct Player {
    pub video: HtmlVideoElement,
    pub state: ArcRwSignal<PlayerState>,
    /// The stream has a sound track.
    pub audio: ArcRwSignal<bool>,
    inner: Rc<RefCell<Inner>>,
}

struct Inner {
    url: String,
    video: HtmlVideoElement,
    state: ArcRwSignal<PlayerState>,
    audio: ArcRwSignal<bool>,
    /// Bumped per connection so late callbacks of an old one are ignored.
    generation: u32,
    conn: Option<Connection>,
    retry: Option<Timeout>,
}

/// Everything belonging to one WebSocket connection.
struct Connection {
    source: MediaSource,
    object_url: String,
    ws: Option<WebSocket>,
    buffer: Option<SourceBuffer>,
    queue: VecDeque<ArrayBuffer>,
    started: bool,
    _handlers: Vec<Closure<dyn FnMut(Event)>>,
}

impl Player {
    pub fn new(url: String) -> Option<Self> {
        let document = web_sys::window()?.document()?;
        let video: HtmlVideoElement = document.create_element("video").ok()?.dyn_into().ok()?;
        video.set_muted(true);
        video.set_autoplay(true);
        video.set_attribute("playsinline", "").ok()?;
        video.set_class_name("preview__video");
        let state = ArcRwSignal::new(PlayerState::Connecting);
        let audio = ArcRwSignal::new(false);
        let inner = Rc::new(RefCell::new(Inner { url, video: video.clone(), state: state.clone(), audio: audio.clone(), generation: 0, conn: None, retry: None }));
        connect(&inner);
        Some(Self { video, state, audio, inner })
    }

    /// Resume after the element was re-attached to the page.
    pub fn resume(&self) {
        if let Ok(inner) = self.inner.try_borrow() {
            jump_to_live(&inner.video);
            play(&inner.video);
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let mut inner = self.inner.borrow_mut();
        inner.retry = None;
        inner.generation += 1;
        if let Some(conn) = inner.conn.take() {
            close(conn, &inner.video);
        }
    }
}

type Shared = Rc<RefCell<Inner>>;

fn connect(shared: &Shared) {
    let Ok(source) = MediaSource::new() else {
        set_state(shared, PlayerState::Unsupported("this browser has no Media Source Extensions".into()));
        return;
    };
    let Ok(object_url) = Url::create_object_url_with_source(&source) else { return };
    let mut inner = shared.borrow_mut();
    inner.generation += 1;
    let generation = inner.generation;
    let weak = Rc::downgrade(shared);
    let on_open = handler(&weak, generation, |shared, _| open_socket(shared));
    source.add_event_listener_with_callback("sourceopen", on_open.as_ref().unchecked_ref()).ok();
    inner.video.set_src(&object_url);
    inner.conn = Some(Connection { source, object_url, ws: None, buffer: None, queue: VecDeque::new(), started: false, _handlers: vec![on_open] });
}

fn open_socket(shared: &Shared) {
    let mut inner = shared.borrow_mut();
    let generation = inner.generation;
    let Ok(ws) = WebSocket::new(&inner.url) else {
        drop(inner);
        schedule_reconnect(shared);
        return;
    };
    ws.set_binary_type(BinaryType::Arraybuffer);
    let weak = Rc::downgrade(shared);
    let on_message = handler(&weak, generation, |shared, e| on_message(shared, e.unchecked_into()));
    let on_close = handler(&weak, generation, |shared, _| {
        if !matches!(shared.borrow().state.get_untracked(), PlayerState::Offline(_) | PlayerState::Unsupported(_)) {
            set_state(shared, PlayerState::Connecting);
        }
        schedule_reconnect(shared);
    });
    ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));
    if let Some(conn) = inner.conn.as_mut() {
        conn.ws = Some(ws);
        conn._handlers.extend([on_message, on_close]);
    }
}

fn on_message(shared: &Shared, e: MessageEvent) {
    let data = e.data();
    if let Some(buf) = data.dyn_ref::<ArrayBuffer>() {
        if matches!(shared.borrow().state.get_untracked(), PlayerState::Unsupported(_)) {
            return;
        }
        if let Some(conn) = shared.borrow_mut().conn.as_mut() {
            conn.queue.push_back(buf.clone());
        }
        pump(shared);
        return;
    }
    let Some(text) = data.as_string() else { return };
    let Ok(msg) = serde_json::from_str::<Control>(&text) else { return };
    match (msg.kind.as_str(), msg.state.as_deref()) {
        ("init", _) => on_init(shared, msg.codec.unwrap_or_default(), msg.audio),
        ("status", Some("offline")) => set_state(shared, PlayerState::Offline(msg.reason.unwrap_or_else(|| "camera unreachable".into()))),
        ("status", _) => set_state(shared, PlayerState::Connecting),
        _ => {}
    }
}

/// Create the source buffer for the stream's codec (once per connection).
fn on_init(shared: &Shared, codec: String, audio: bool) {
    let mime = format!("video/mp4; codecs=\"{codec}\"");
    if !MediaSource::is_type_supported(&mime) {
        if let Some(conn) = shared.borrow_mut().conn.as_mut() {
            conn.queue.clear();
        }
        let reason = if codec.starts_with("hvc1.") {
            "this browser can't play HEVC (H.265); use an HEVC-capable browser or an H.264 camera stream".to_string()
        } else {
            format!("this browser can't play {codec}")
        };
        set_state(shared, PlayerState::Unsupported(reason));
        return;
    }
    set_state(shared, PlayerState::Connecting);
    let mut inner = shared.borrow_mut();
    if inner.audio.get_untracked() != audio {
        inner.audio.set(audio);
    }
    let generation = inner.generation;
    let weak = Rc::downgrade(shared);
    let Some(conn) = inner.conn.as_mut() else { return };
    conn.started = false;
    conn.queue.clear();
    match &conn.buffer {
        Some(buffer) => {
            if buffer.change_type(&mime).is_err() {
                // A busy buffer or a codec switch may require a fresh MediaSource.
                drop(inner);
                schedule_reconnect(shared);
            }
        }
        None => match conn.source.add_source_buffer(&mime) {
            Ok(buffer) => {
                buffer.set_mode(SourceBufferAppendMode::Segments);
                let on_done = handler(&weak, generation, |shared, _| pump(shared));
                buffer.add_event_listener_with_callback("updateend", on_done.as_ref().unchecked_ref()).ok();
                conn._handlers.push(on_done);
                conn.buffer = Some(buffer);
            }
            Err(_) => {
                drop(inner);
                set_state(shared, PlayerState::Unsupported(format!("cannot open a {codec} decoder")));
            }
        },
    }
}

/// Append the next queued segment, or tidy up when the queue is empty.
fn pump(shared: &Shared) {
    let Ok(mut inner) = shared.try_borrow_mut() else { return };
    let video = inner.video.clone();
    let Some(conn) = inner.conn.as_mut() else { return };
    let Some(buffer) = conn.buffer.clone() else { return };
    if buffer.updating() {
        return;
    }
    if let Some(next) = conn.queue.pop_front() {
        if buffer.append_buffer_with_array_buffer(&next).is_err() {
            // Usually QuotaExceeded: drop the backlog and free space.
            conn.queue.clear();
            trim(&buffer, &video, 0.0);
        }
        return;
    }
    let first = !conn.started && video.buffered().length() > 0;
    if first {
        conn.started = true;
    }
    drop(inner);
    if first {
        set_state(shared, PlayerState::Playing);
        play(&video);
    }
    jump_to_live(&video);
    trim(&buffer, &video, TRIM_AFTER);
}

fn jump_to_live(video: &HtmlVideoElement) {
    let ranges = video.buffered();
    let n = ranges.length();
    if n == 0 {
        return;
    }
    let (Ok(start), Ok(end)) = (ranges.start(n - 1), ranges.end(n - 1)) else { return };
    let now = video.current_time();
    if now < start || end - now > MAX_LATENCY {
        video.set_current_time((end - CATCH_UP_TO).max(start));
    }
}

/// Remove history older than `KEEP_BEHIND` once more than `after` seconds pile up.
fn trim(buffer: &SourceBuffer, video: &HtmlVideoElement, after: f64) {
    let ranges = video.buffered();
    if buffer.updating() || ranges.length() == 0 {
        return;
    }
    let Ok(start) = ranges.start(0) else { return };
    let now = video.current_time();
    if now - start > after.max(KEEP_BEHIND) {
        let _ = buffer.remove(start, now - KEEP_BEHIND);
    }
}

thread_local! {
    static IGNORE: Closure<dyn FnMut(JsValue)> = Closure::new(|_| {});
}

fn play(video: &HtmlVideoElement) {
    if let Ok(p) = video.play() {
        // Autoplay may be refused (e.g. background tab); that's fine.
        IGNORE.with(|ignore| {
            let _ = p.catch(ignore);
        });
    }
}

fn schedule_reconnect(shared: &Shared) {
    let mut inner = shared.borrow_mut();
    if let Some(conn) = inner.conn.take() {
        close(conn, &inner.video);
    }
    inner.generation += 1;
    let weak = Rc::downgrade(shared);
    inner.retry = Some(Timeout::new(RECONNECT_MS, move || {
        if let Some(shared) = weak.upgrade() {
            shared.borrow_mut().retry = None;
            connect(&shared);
        }
    }));
}

fn close(conn: Connection, video: &HtmlVideoElement) {
    if let Some(ws) = &conn.ws {
        ws.set_onmessage(None);
        ws.set_onclose(None);
        let _ = ws.close();
    }
    let _ = video.remove_attribute("src");
    video.load();
    let _ = Url::revoke_object_url(&conn.object_url);
}

fn set_state(shared: &Shared, state: PlayerState) {
    let signal = shared.borrow().state.clone();
    if signal.get_untracked() != state {
        signal.set(state);
    }
}

/// Event handler bound to one connection generation.
fn handler(weak: &Weak<RefCell<Inner>>, generation: u32, f: impl Fn(&Shared, Event) + 'static) -> Closure<dyn FnMut(Event)> {
    let weak = weak.clone();
    Closure::new(move |e: Event| {
        let Some(shared) = weak.upgrade() else { return };
        let current = shared.try_borrow().map(|i| i.generation == generation).unwrap_or(false);
        if current {
            f(&shared, e);
        }
    })
}
