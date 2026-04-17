//! Integration tests for keyboard shortcuts
//!
//! Tests the keyboard shortcut system with the TUI configuration

#[cfg(test)]
mod tests {
    use rustycode_tui::app::KeyboardShortcutHandler;
    use rustycode_tui::config::BehaviorConfig;

    #[test]
    fn test_keyboard_handler_with_vim_mode_enabled() {
        let mut handler = KeyboardShortcutHandler::new(true);
        assert!(handler.is_vim_enabled());

        // Test that Vim keys work when enabled
        let action = handler.handle_vim_key('j');
        assert_eq!(
            action,
            rustycode_tui::app::KeyboardAction::MoveDown,
            "j should move down in Vim mode"
        );

        let action = handler.handle_vim_key('k');
        assert_eq!(
            action,
            rustycode_tui::app::KeyboardAction::MoveUp,
            "k should move up in Vim mode"
        );
    }

    #[test]
    fn test_keyboard_handler_with_vim_mode_disabled() {
        let handler = KeyboardShortcutHandler::new(false);
        assert!(!handler.is_vim_enabled());

        // Handler can still handle keys, but the event loop will check is_vim_enabled()
        // before calling handle_vim_key
    }

    #[test]
    fn test_behavior_config_vim_setting() {
        let config = BehaviorConfig {
            auto_save_interval_seconds: 30,
            max_history_size: 1000,
            confirm_on_dangerous: true,
            yolo_mode: false,
            auto_scroll: true,
            stream_responses: true,
            mouse_scroll_speed: 3,
            vim_enabled: true,
            reduced_motion: false,
        };

        assert!(config.vim_enabled);
    }

    #[test]
    fn test_vim_chord_detection_in_handler() {
        let mut handler = KeyboardShortcutHandler::new(true);

        // First 'g' should not trigger action
        let action = handler.handle_vim_key('g');
        assert_eq!(action, rustycode_tui::app::KeyboardAction::None);
        assert!(handler.vim_chord_state.pending_g);

        // Second 'g' should trigger jump to start
        let action = handler.handle_vim_key('g');
        assert_eq!(action, rustycode_tui::app::KeyboardAction::JumpToStart);
        assert!(!handler.vim_chord_state.pending_g);
    }

    #[test]
    fn test_handler_reset_clears_state() {
        let mut handler = KeyboardShortcutHandler::new(true);

        // Set up state
        handler.handle_vim_key('g');
        assert!(handler.vim_chord_state.pending_g);

        // Reset should clear state
        handler.reset();
        assert!(!handler.vim_chord_state.pending_g);
    }
}
