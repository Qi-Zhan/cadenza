use cadenza::app::PlaybackState;

#[test]
fn toggles_pause_and_resume() {
    let mut state = PlaybackState::Playing;

    state.toggle_pause();
    assert!(matches!(state, PlaybackState::Paused));

    state.toggle_pause();
    assert!(matches!(state, PlaybackState::Playing));
}

#[test]
fn stopped_state_does_not_toggle_into_playing() {
    let mut state = PlaybackState::Stopped;

    state.toggle_pause();
    assert!(matches!(state, PlaybackState::Stopped));
}
