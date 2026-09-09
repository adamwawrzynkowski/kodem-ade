//! Desktop-owned window placement, restored before the host connects.
use gpui::*;
use serde_json::{Value, json};

/// Restores usable window bounds, recovering onto a connected display when necessary.
pub fn load(cx: &App) -> WindowBounds {
    let saved = kodem_core::ipc::data_dir()
        .ok()
        .and_then(|root| std::fs::read(root.join("window-state.json")).ok())
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|value| decode(&value));
    let Some(saved) = saved else {
        return WindowBounds::centered(size(px(1512.), px(982.)), cx);
    };
    let bounds = saved.get_bounds();
    let displays = cx.displays();
    let display = displays
        .iter()
        .find(|display| {
            display
                .bounds()
                .contains(&point(bounds.center().x, bounds.top() + px(20.)))
        })
        .or_else(|| displays.first());
    let Some(display) = display else {
        return saved;
    };
    let screen = display.bounds();
    let width = bounds.size.width.min(screen.size.width);
    let height = bounds.size.height.min(screen.size.height);
    let restored = Bounds::new(
        point(
            bounds.left().max(screen.left()).min(screen.right() - width),
            bounds.top().max(screen.top()).min(screen.bottom() - height),
        ),
        size(width, height),
    );
    match saved {
        WindowBounds::Windowed(_) => WindowBounds::Windowed(restored),
        WindowBounds::Maximized(_) => WindowBounds::Maximized(restored),
        WindowBounds::Fullscreen(_) => WindowBounds::Fullscreen(restored),
    }
}

fn decode(value: &Value) -> Option<WindowBounds> {
    let coordinates = ["x", "y", "width", "height"].map(|key| value[key].as_f64());
    let [Some(x), Some(y), Some(width), Some(height)] = coordinates else {
        return None;
    };
    if ![x, y, width, height]
        .iter()
        .all(|n| n.is_finite() && n.abs() <= 100_000.)
    {
        return None;
    }
    if width < 850. || height < 600. {
        return None;
    }
    let bounds = Bounds::new(
        point(px(x as f32), px(y as f32)),
        size(px(width as f32), px(height as f32)),
    );
    match value["state"].as_str()? {
        "windowed" => Some(WindowBounds::Windowed(bounds)),
        "maximized" => Some(WindowBounds::Maximized(bounds)),
        "fullscreen" => Some(WindowBounds::Fullscreen(bounds)),
        _ => None,
    }
}

/// Atomically saves the latest placement without writing to the host-owned database.
pub fn save(placement: WindowBounds) -> anyhow::Result<()> {
    let root = kodem_core::ipc::data_dir()?;
    std::fs::create_dir_all(&root)?;
    let bounds = placement.get_bounds();
    let value = json!({
        "x": f32::from(bounds.left()), "y": f32::from(bounds.top()),
        "width": f32::from(bounds.size.width), "height": f32::from(bounds.size.height),
        "state": match placement {
            WindowBounds::Windowed(_) => "windowed",
            WindowBounds::Maximized(_) => "maximized",
            WindowBounds::Fullscreen(_) => "fullscreen",
        }
    });
    let temporary = root.join(format!("window-state.{}.tmp", std::process::id()));
    std::fs::write(&temporary, serde_json::to_vec(&value)?)?;
    if let Err(error) = std::fs::rename(&temporary, root.join("window-state.json")) {
        let _ = std::fs::remove_file(temporary);
        return Err(error.into());
    }
    Ok(())
}
