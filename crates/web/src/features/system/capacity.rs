use leptos::prelude::*;

use crate::api::{CapacityEstimate, Resource};
use crate::ui::{I, Icon, Meter, Tone};

fn label(r: Resource) -> &'static str {
    match r {
        Resource::Cpu => "CPU",
        Resource::Memory => "Memory",
        Resource::DiskWrite => "Disk write",
        Resource::Network => "Network",
    }
}

fn icon(r: Resource) -> I {
    match r {
        Resource::Cpu => I::Cpu,
        Resource::Memory => I::MemoryStick,
        Resource::DiskWrite => I::HardDrive,
        Resource::Network => I::ArrowDownUp,
    }
}

/// "5 of ~24 cameras" plus projected load per resource and advice.
#[component]
pub fn CapacitySummary(estimate: CapacityEstimate) -> impl IntoView {
    let used = estimate.cameras as f32 / estimate.max_cameras.max(1) as f32 * 100.0;
    let plural = |n: u32| if n == 1 { "" } else { "s" };
    let verdict = match estimate.cameras.cmp(&estimate.max_cameras) {
        std::cmp::Ordering::Less => {
            let n = estimate.max_cameras - estimate.cameras;
            format!("Room for about {n} more camera{} with similar settings", plural(n))
        }
        std::cmp::Ordering::Equal => "At capacity — adding cameras may cause dropped frames".to_string(),
        std::cmp::Ordering::Greater => {
            let n = estimate.cameras - estimate.max_cameras;
            format!("Over the estimate by ~{n} camera{} — expect dropped frames or missed events", plural(n))
        }
    };
    let tone = |p: f32| if p >= 90.0 { Tone::Danger } else if p >= 70.0 { Tone::Warning } else { Tone::Online };
    let hw = &estimate.hardware;

    view! {
        <div class="capacity">
            <div class="capacity__head">
                <span class="capacity__count">{estimate.cameras}<span class="capacity__of">{format!(" of ~{}", estimate.max_cameras)}</span></span>
                <span class="capacity__unit">"cameras"</span>
            </div>
            <Meter value=used tone=tone(used) />
            <p class="capacity__verdict">{verdict}</p>
            <ul class="capacity__resources">
                {estimate.resources.into_iter().map(|r| {
                    let bottleneck = r.resource == estimate.bottleneck;
                    view! {
                        <li class="capacity__row" class:capacity__row--limit=bottleneck title=r.detail.clone()>
                            <span class="capacity__label"><Icon icon=icon(r.resource) class="icon icon--sm" />{label(r.resource)}</span>
                            <Meter value=r.percent.min(100.0) tone=tone(r.percent) />
                            <span class="capacity__pct">{format!("{:.0}%", r.percent)}</span>
                        </li>
                    }
                }).collect_view()}
            </ul>
            <p class="capacity__hw">
                {format!("{} · {} cores{}", hw.cpu_model, hw.cpu_cores, if hw.hw_decode { " · HW decode" } else { "" })}
            </p>
            {(!estimate.advice.is_empty()).then(|| view! {
                <ul class="capacity__advice">
                    {estimate.advice.into_iter().map(|a| view! { <li><Icon icon=I::TriangleAlert class="icon icon--sm" />{a}</li> }).collect_view()}
                </ul>
            })}
        </div>
    }
}
