//! Save the frame currently shown by a `<video>` as a JPEG download.
//! Done in the browser (which already decodes the stream), so the server
//! never needs a video decoder.

use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, Element, HtmlAnchorElement, HtmlCanvasElement, HtmlVideoElement};

/// Capture the first playing video inside `container`. `name` becomes the
/// file name (without extension).
pub fn save_frame(container: &Element, name: &str) -> Result<(), &'static str> {
    let video: HtmlVideoElement = container.query_selector("video").ok().flatten().and_then(|v| v.dyn_into().ok()).ok_or("No live video to capture")?;
    let (w, h) = (video.video_width(), video.video_height());
    if w == 0 || h == 0 {
        return Err("The video hasn't started yet");
    }
    let document = web_sys::window().and_then(|w| w.document()).ok_or("No document")?;
    let canvas: HtmlCanvasElement = document.create_element("canvas").map_err(|_| "Cannot create a canvas")?.dyn_into().map_err(|_| "Cannot create a canvas")?;
    canvas.set_width(w);
    canvas.set_height(h);
    let ctx: CanvasRenderingContext2d = canvas.get_context("2d").ok().flatten().and_then(|c| c.dyn_into().ok()).ok_or("Canvas unavailable")?;
    ctx.draw_image_with_html_video_element(&video, 0.0, 0.0).map_err(|_| "Cannot read the video frame")?;
    let url = canvas.to_data_url_with_type("image/jpeg").map_err(|_| "Cannot encode the image")?;
    let link: HtmlAnchorElement = document.create_element("a").map_err(|_| "Cannot save")?.dyn_into().map_err(|_| "Cannot save")?;
    link.set_href(&url);
    link.set_download(&format!("{name}.jpg"));
    link.click();
    Ok(())
}

/// `<camera>_<YYYY-MM-DD_HH-MM-SS>` in local time, safe as a file name.
pub fn file_name(camera: &str) -> String {
    let safe: String = camera.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    format!("{safe}_{}", chrono::Local::now().format("%Y-%m-%d_%H-%M-%S"))
}
