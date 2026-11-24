//! Frame-based input state, independent of any windowing backend.

use std::collections::BTreeSet;

/// A stable logical input button.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Button {
    /// Move forward.
    MoveForward,
    /// Move backward.
    MoveBackward,
    /// Move left.
    MoveLeft,
    /// Move right.
    MoveRight,
    /// Jump.
    Jump,
    /// Sprint.
    Sprint,
    /// Interact.
    Interact,
}

/// Current and edge-triggered input state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InputState {
    held: BTreeSet<Button>,
    pressed: BTreeSet<Button>,
    released: BTreeSet<Button>,
}

impl InputState {
    /// Starts a new input frame, clearing transient edges but preserving held buttons.
    pub fn begin_frame(&mut self) {
        self.pressed.clear();
        self.released.clear();
    }

    /// Records a button transition. Repeated key-down events do not retrigger the pressed edge.
    pub fn set_button(&mut self, button: Button, is_down: bool) {
        if is_down {
            if self.held.insert(button) { self.pressed.insert(button); }
        } else if self.held.remove(&button) {
            self.released.insert(button);
        }
    }

    /// Whether the button is currently held.
    #[must_use]
    pub fn is_held(&self, button: Button) -> bool { self.held.contains(&button) }

    /// Whether the button became held during this frame.
    #[must_use]
    pub fn was_pressed(&self, button: Button) -> bool { self.pressed.contains(&button) }

    /// Whether the button became released during this frame.
    #[must_use]
    pub fn was_released(&self, button: Button) -> bool { self.released.contains(&button) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transitions_are_edges_and_held_state_persists() {
        let mut input = InputState::default();
        input.set_button(Button::Jump, true);
        input.set_button(Button::Jump, true);
        assert!(input.is_held(Button::Jump));
        assert!(input.was_pressed(Button::Jump));
        input.begin_frame();
        assert!(input.is_held(Button::Jump));
        assert!(!input.was_pressed(Button::Jump));
        input.set_button(Button::Jump, false);
        assert!(input.was_released(Button::Jump));
        assert!(!input.is_held(Button::Jump));
    }
}
