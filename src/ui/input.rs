//! Unified menu navigation from keyboard and gamepad (gamepad-first UX).

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

/// Read-only view of "menu intents" this frame, merged across keyboard and all gamepads
#[derive(SystemParam)]
pub struct MenuInput<'w, 's> {
    keyboard: Res<'w, ButtonInput<KeyCode>>,
    gamepads: Query<'w, 's, &'static Gamepad>,
}

impl MenuInput<'_, '_> {
    fn any(&self, keys: &[KeyCode], buttons: &[GamepadButton]) -> bool {
        self.keyboard.any_just_pressed(keys.iter().copied())
            || self.gamepads.iter().any(|pad| pad.any_just_pressed(buttons.iter().copied()))
    }

    pub fn up(&self) -> bool {
        self.any(&[KeyCode::ArrowUp, KeyCode::KeyW], &[GamepadButton::DPadUp])
    }

    pub fn down(&self) -> bool {
        self.any(&[KeyCode::ArrowDown, KeyCode::KeyS], &[GamepadButton::DPadDown])
    }

    pub fn left(&self) -> bool {
        self.any(&[KeyCode::ArrowLeft, KeyCode::KeyA], &[GamepadButton::DPadLeft])
    }

    pub fn right(&self) -> bool {
        self.any(&[KeyCode::ArrowRight, KeyCode::KeyD], &[GamepadButton::DPadRight])
    }

    pub fn confirm(&self) -> bool {
        self.any(&[KeyCode::Enter, KeyCode::Space], &[GamepadButton::South])
    }

    pub fn back(&self) -> bool {
        self.any(&[KeyCode::Escape, KeyCode::Backspace], &[GamepadButton::East])
    }

    /// Next column/tab (Tab, or right bumper)
    pub fn next_section(&self) -> bool {
        self.any(&[KeyCode::Tab], &[GamepadButton::RightTrigger])
    }

    /// Step a selection index through `len` items with up/down, wrapping around
    pub fn step_vertical(&self, index: &mut usize, len: usize) -> bool {
        step(index, len, self.up(), self.down())
    }

    /// Step a selection index through `len` items with left/right, wrapping around
    pub fn step_horizontal(&self, index: &mut usize, len: usize) -> bool {
        step(index, len, self.left(), self.right())
    }
}

fn step(index: &mut usize, len: usize, prev: bool, next: bool) -> bool {
    if len == 0 {
        return false;
    }
    if prev {
        *index = (*index + len - 1) % len;
        true
    } else if next {
        *index = (*index + 1) % len;
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::step;

    #[test]
    fn step_wraps_both_ways() {
        let mut i = 0;
        step(&mut i, 3, true, false);
        assert_eq!(i, 2);
        step(&mut i, 3, false, true);
        assert_eq!(i, 0);
        assert!(!step(&mut i, 0, true, false));
    }
}
