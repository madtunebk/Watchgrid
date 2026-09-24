//! Bar under the wall: page navigation, auto-cycle, and page load.

use leptos::prelude::*;

use super::budget::{Level, PageLoad};
use crate::ui::{I, Icon};

#[component]
pub fn WallFooter(
    page: RwSignal<usize>,
    #[prop(into)] pages: Signal<usize>,
    cycle: RwSignal<bool>,
    #[prop(into)] load: Signal<PageLoad>,
) -> impl IntoView {
    let step = move |delta: isize| {
        let n = pages.get_untracked().max(1) as isize;
        page.set(((page.get_untracked() as isize + delta).rem_euclid(n)) as usize);
    };

    view! {
        <div class="wall-footer">
            <div class="wall-footer__load">
                {move || {
                    let l = load.get();
                    let (class, text) = match l.level {
                        Level::Light => ("load load--light", "Light"),
                        Level::Moderate => ("load load--moderate", "Moderate"),
                        Level::Heavy => ("load load--heavy", "Heavy"),
                    };
                    view! {
                        <span class=class title=format!("Decoding ≈ {:.1} × 1080p@25 in this browser", l.decode)>
                            <span class="load__dot"></span>{format!("Browser load: {text}")}
                        </span>
                        <span class="muted">{format!("{} live · {:.1} Mbit/s{}", l.streams, l.bitrate as f32 / 1000.0,
                            if l.using_substreams { " · substreams" } else { "" })}</span>
                        {(l.level == Level::Heavy).then(|| view! {
                            <span class="load__tip">"Use a smaller grid or add substreams to the cameras"</span>
                        })}
                    }
                }}
            </div>

            <Show when=move || { pages.get() > 1 }>
                <div class="pager" role="navigation" aria-label="Live view pages">
                    <button class="icon-btn icon-btn--sm" aria-label="Previous page" on:click=move |_| step(-1)>
                        <Icon icon=I::ChevronLeft />
                    </button>
                    <span class="pager__label">{move || format!("Page {} / {}", page.get() + 1, pages.get())}</span>
                    <button class="icon-btn icon-btn--sm" aria-label="Next page" on:click=move |_| step(1)>
                        <Icon icon=I::ChevronRight />
                    </button>
                    <button class="btn btn--sm" class:btn--primary=cycle class:btn--secondary=move || !cycle.get()
                        title="Show each page in turn every 15 s" aria-pressed=move || cycle.get().to_string()
                        on:click=move |_| cycle.update(|c| *c = !*c)>
                        {move || view! { <Icon icon=if cycle.get() { I::Pause } else { I::Play } class="icon icon--sm" /> }}
                        "Cycle"
                    </button>
                </div>
            </Show>

            <p class="wall-footer__hint">"Drag to rearrange · double-click: single view · 1–4: layout · ← →: page · Esc: back"</p>
        </div>
    }
}
