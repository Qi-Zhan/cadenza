use cadenza::player::extract_analysis_window;

#[test]
fn extracts_recent_window_ending_at_cursor() {
    let samples = vec![0.0, 1.0, 2.0, 3.0, 4.0];
    let window = extract_analysis_window(&samples, 4, 3);

    assert_eq!(window, vec![1.0, 2.0, 3.0]);
}

#[test]
fn clamps_window_at_start_of_track() {
    let samples = vec![0.0, 1.0, 2.0];
    let window = extract_analysis_window(&samples, 1, 4);

    assert_eq!(window, vec![0.0]);
}
