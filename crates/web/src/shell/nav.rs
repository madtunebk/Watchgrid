use crate::ui::I;

pub struct NavItem {
    pub href: &'static str,
    pub label: &'static str,
    pub icon: I,
}

pub const PRIMARY: &[NavItem] = &[
    NavItem { href: "/", label: "Dashboard", icon: I::LayoutDashboard },
    NavItem { href: "/cameras", label: "Cameras", icon: I::Cctv },
    NavItem { href: "/live", label: "Live View", icon: I::MonitorPlay },
    NavItem { href: "/events", label: "Events", icon: I::Activity },
    NavItem { href: "/recordings", label: "Recordings", icon: I::Film },
];

pub const ADMIN: &[NavItem] = &[
    NavItem { href: "/storage", label: "Storage", icon: I::HardDrive },
    NavItem { href: "/system", label: "System", icon: I::Cpu },
    NavItem { href: "/settings", label: "Settings", icon: I::Settings },
];

/// Breadcrumb trail for a path: (label, href).
pub fn crumbs(path: &str) -> Vec<(String, String)> {
    let segs: Vec<&str> = path.trim_matches('/').split('/').filter(|s| !s.is_empty()).collect();
    let top = |label: &str, href: &str| (label.to_string(), href.to_string());
    let section = |s: &str| {
        PRIMARY.iter().chain(ADMIN).find(|n| n.href.trim_start_matches('/') == s).map(|n| top(n.label, n.href))
    };

    match segs.as_slice() {
        [] => vec![top("Dashboard", "/")],
        ["cameras", "new"] => vec![top("Cameras", "/cameras"), top("Add camera", path)],
        ["cameras", id, "edit"] => {
            vec![top("Cameras", "/cameras"), top("Camera", &format!("/cameras/{id}")), top("Edit", path)]
        }
        ["cameras", id, ..] => vec![top("Cameras", "/cameras"), top("Camera", &format!("/cameras/{id}"))],
        ["events", _] => vec![top("Events", "/events"), top("Event", path)],
        // Reached from the bell, not the sidebar.
        ["notifications"] => vec![top("Notifications", path)],
        ["settings", sub, ..] => {
            let label = crate::features::settings::section_label(sub).map(String::from).unwrap_or_else(|| title_case(sub));
            vec![top("Settings", "/settings"), top(&label, path)]
        }
        [s] => section(s).map(|c| vec![c]).unwrap_or_else(|| vec![top("Not found", path)]),
        _ => vec![top("Not found", path)],
    }
}

fn title_case(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}
