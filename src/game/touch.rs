//! Touch controls for phones and tablets.
//!
//! A finger works like the mouse, so the board needs no touch-specific rules:
//! - putting a finger down hovers there (the cursor moves, a route draws);
//! - lifting it again without dragging is a tap, a left click at that spot
//!   (select a unit, move it, attack, open a workshop);
//! - dragging one finger pans the camera, pinching two zooms.
//!
//! Touches that start on a menu or panel belong to egui, which handles touch
//! itself, and are left alone here.

use std::collections::HashMap;

use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;
use bevy::window::CursorMoved;
use bevy_egui::{egui, EguiContexts};

use super::{BoardPointer, CameraAngle, CameraZoom, ModalOpen};

pub struct TouchControlsPlugin;

impl Plugin for TouchControlsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TouchGesture>()
            .add_systems(PreUpdate, touch_gestures
                .after(bevy::input::InputSystems)
                .before(super::movement::update_pointer));
    }
}

/// How far (in logical pixels) a finger may wander and still count as a tap
const TAP_SLOP: f32 = 14.0;
/// A touch held longer than this isn't a tap
const TAP_SECS: f64 = 0.6;

struct Finger {
    start: Vec2,
    last: Vec2,
    started: f64,
}

#[derive(Resource, Default)]
struct TouchGesture {
    fingers: HashMap<u64, Finger>,
    /// The gesture began on a menu or panel: egui's, not the board's
    on_ui: bool,
    panning: bool,
    pinching: bool,
    pinch_distance: f32,
}

fn touch_gestures(
    mut touches: MessageReader<TouchInput>,
    mut gesture: ResMut<TouchGesture>,
    mut pointer: ResMut<BoardPointer>,
    mut cursor_moved: MessageWriter<CursorMoved>,
    mut contexts: EguiContexts,
    modal: Res<ModalOpen>,
    time: Res<Time>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut angle: ResMut<CameraAngle>,
    mut zoom: ResMut<CameraZoom>,
    ui_scale: Query<&bevy_egui::EguiContextSettings>,
) {
    pointer.tapped = false;
    let now = time.elapsed_secs_f64();
    for touch in touches.read() {
        match touch.phase {
            TouchPhase::Started => {
                if gesture.fingers.is_empty() {
                    // A gesture starting over an interactive panel or under a
                    // modal menu is for the UI
                    let over_ui = contexts.ctx_mut().ok().is_some_and(|ctx| {
                        // egui points are logical pixels divided by the UI scale
                        let scale = ui_scale.iter().next().map_or(1.0, |s| s.scale_factor);
                        let pos = egui::pos2(touch.position.x, touch.position.y) / scale;
                        ctx.layer_id_at(pos).is_some_and(|layer| layer.order != egui::Order::Background)
                    });
                    *gesture = TouchGesture { on_ui: over_ui || modal.0, ..default() };
                }
                gesture.fingers.insert(touch.id, Finger { start: touch.position, last: touch.position, started: now });
                match gesture.fingers.len() {
                    1 if !gesture.on_ui => {
                        // Hover where the finger lands, like moving the mouse there
                        cursor_moved.write(CursorMoved { window: touch.window, position: touch.position, delta: None });
                    }
                    2 => {
                        gesture.pinching = true;
                        gesture.pinch_distance = finger_spread(&gesture.fingers);
                    }
                    _ => {}
                }
            }
            TouchPhase::Moved => {
                let Some(finger) = gesture.fingers.get_mut(&touch.id) else { continue };
                let last = std::mem::replace(&mut finger.last, touch.position);
                let start = finger.start;
                if gesture.on_ui {
                    continue;
                }
                if gesture.pinching && gesture.fingers.len() >= 2 {
                    // Fingers apart zoom in, together zoom out
                    let spread = finger_spread(&gesture.fingers);
                    if gesture.pinch_distance > 1.0 && spread > 1.0 {
                        zoom.current = (zoom.current * gesture.pinch_distance / spread).clamp(zoom.min, zoom.max);
                    }
                    gesture.pinch_distance = spread;
                } else if gesture.fingers.len() == 1 {
                    if !gesture.panning && start.distance(touch.position) > TAP_SLOP {
                        gesture.panning = true;
                    }
                    if gesture.panning {
                        // Drag the board along under the finger
                        if let Ok((camera, transform)) = cameras.single() {
                            if let (Some(from), Some(to)) = (ground_point(camera, transform, last), ground_point(camera, transform, touch.position)) {
                                angle.look_at += from - to;
                            }
                        }
                    }
                }
            }
            TouchPhase::Ended | TouchPhase::Canceled => {
                let Some(finger) = gesture.fingers.remove(&touch.id) else { continue };
                let tap = touch.phase == TouchPhase::Ended
                    && !gesture.on_ui && !gesture.panning && !gesture.pinching
                    && gesture.fingers.is_empty()
                    && finger.start.distance(touch.position) <= TAP_SLOP
                    && now - finger.started <= TAP_SECS;
                if tap {
                    cursor_moved.write(CursorMoved { window: touch.window, position: touch.position, delta: None });
                    pointer.position = Some(touch.position);
                    pointer.tapped = true;
                }
            }
        }
    }
}

/// Distance between the first two fingers
fn finger_spread(fingers: &HashMap<u64, Finger>) -> f32 {
    let mut points = fingers.values().map(|f| f.last);
    match (points.next(), points.next()) {
        (Some(a), Some(b)) => a.distance(b),
        _ => 0.0,
    }
}

/// Where a screen position meets the ground plane, as XZ
fn ground_point(camera: &Camera, transform: &GlobalTransform, screen: Vec2) -> Option<Vec2> {
    let ray = camera.viewport_to_world(transform, screen).ok()?;
    if ray.direction.y.abs() < 1e-4 {
        return None;
    }
    let t = -ray.origin.y / ray.direction.y;
    (t > 0.0).then(|| {
        let hit = ray.origin + ray.direction * t;
        Vec2::new(hit.x, hit.z)
    })
}
