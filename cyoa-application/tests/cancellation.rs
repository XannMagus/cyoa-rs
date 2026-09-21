use cyoa_application::cancellation::CancellationSource;

#[test]
fn cancellation_is_shared_across_threads_and_isolated_between_requests() {
    let source = CancellationSource::default();
    let token = source.token();
    let other = CancellationSource::default();
    assert!(!token.is_cancelled());
    source.cancel();
    std::thread::spawn(move || assert!(token.is_cancelled()))
        .join()
        .unwrap();
    assert!(source.token().is_cancelled());
    assert!(!other.token().is_cancelled());
}
