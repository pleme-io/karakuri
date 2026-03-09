use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::msg_send;
use objc2_app_kit::{NSBezierPath, NSColor, NSFont};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

use super::components::{ArgbColor, BarItemGeometry, BarItemState};

/// Draw all visible bar items into the bar's content view.
///
/// Called from the Bevy render system when `StatusBarState.needs_redraw` is true.
/// Operates on the main thread (NSGraphicsContext is not thread-safe).
pub fn draw_bar_items(
    items: &[(BarItemState, BarItemGeometry)],
    bar_height: f64,
    default_font_name: &str,
    default_font_size: f64,
) {
    for (state, geom) in items {
        if state.hidden {
            continue;
        }

        // Draw background if not fully transparent
        if state.background_color.a > 0.001 {
            draw_rounded_rect(
                geom.x,
                (bar_height - geom.height) / 2.0,
                geom.width,
                geom.height,
                state.corner_radius,
                &state.background_color,
                &state.border_color,
                state.border_width,
            );
        }

        // Draw icon (left side of item)
        let mut text_x = geom.x + geom.padding_left;
        if !state.icon.is_empty() {
            draw_text_simple(
                &state.icon,
                text_x,
                bar_height,
                default_font_name,
                default_font_size,
                &state.icon_color,
            );
            let icon_advance = default_font_size * 0.8 * state.icon.chars().count() as f64;
            text_x += icon_advance + 4.0;
        }

        // Draw label (right of icon)
        if !state.label.is_empty() {
            draw_text_simple(
                &state.label,
                text_x,
                bar_height,
                default_font_name,
                default_font_size,
                &state.label_color,
            );
        }
    }
}

/// Draw a rounded rectangle with optional fill and border.
fn draw_rounded_rect(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    radius: f64,
    fill: &ArgbColor,
    border: &ArgbColor,
    border_width: f64,
) {
    let rect = NSRect::new(NSPoint::new(x, y), NSSize::new(width, height));
    let path = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(rect, radius, radius);

    // Fill
    if fill.a > 0.001 {
        let color = NSColor::colorWithSRGBRed_green_blue_alpha(fill.r, fill.g, fill.b, fill.a);
        color.set();
        path.fill();
    }

    // Border
    if border.a > 0.001 && border_width > 0.0 {
        let color =
            NSColor::colorWithSRGBRed_green_blue_alpha(border.r, border.g, border.b, border.a);
        color.set();
        path.setLineWidth(border_width);
        path.stroke();
    }
}

/// Draw text at a position using NSString's drawing API.
///
/// Uses `drawAtPoint:withAttributes:` via raw `msg_send!` with an
/// NSDictionary of attributes. This avoids constructing NSAttributedString
/// (which has complex generic bounds in objc2).
fn draw_text_simple(
    text: &str,
    x: f64,
    bar_height: f64,
    font_name: &str,
    font_size: f64,
    color: &ArgbColor,
) {
    let ns_text = NSString::from_str(text);

    // Get font
    let name = NSString::from_str(font_name);
    let font = NSFont::fontWithName_size(&name, font_size)
        .unwrap_or_else(|| NSFont::systemFontOfSize(font_size));

    // Vertical centering
    let ascender = font.ascender();
    let descender = font.descender();
    let text_height = ascender - descender;
    let y = (bar_height - text_height as f64) / 2.0 - descender as f64;

    let point = NSPoint::new(x, y);
    let ns_color = NSColor::colorWithSRGBRed_green_blue_alpha(color.r, color.g, color.b, color.a);

    // Build attributes dictionary via raw msg_send to avoid NSMutableDictionary generic complexity
    unsafe {
        let dict: Retained<AnyObject> = msg_send![objc2::class!(NSMutableDictionary), new];
        let font_key = objc2_app_kit::NSFontAttributeName;
        let color_key = objc2_app_kit::NSForegroundColorAttributeName;
        let _: () = msg_send![&dict, setObject: &*font, forKey: font_key];
        let _: () = msg_send![&dict, setObject: &*ns_color, forKey: color_key];
        let _: () = msg_send![&ns_text, drawAtPoint: point, withAttributes: &*dict];
    }
}

/// Measure the width of text with the given font (for layout).
pub fn measure_text(text: &str, font_name: &str, font_size: f64) -> f64 {
    if text.is_empty() {
        return 0.0;
    }
    let ns_text = NSString::from_str(text);
    let name = NSString::from_str(font_name);
    let font = NSFont::fontWithName_size(&name, font_size)
        .unwrap_or_else(|| NSFont::systemFontOfSize(font_size));

    // Use sizeWithAttributes: to measure text width
    unsafe {
        let dict: Retained<AnyObject> = msg_send![objc2::class!(NSMutableDictionary), new];
        let font_key = objc2_app_kit::NSFontAttributeName;
        let _: () = msg_send![&dict, setObject: &*font, forKey: font_key];
        let size: NSSize = msg_send![&ns_text, sizeWithAttributes: &*dict];
        size.width
    }
}
