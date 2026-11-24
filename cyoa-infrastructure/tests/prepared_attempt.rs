use cyoa_application::persistence::*;
use cyoa_infrastructure::persistence::prepared::{PreparedWriteBytes, prepare_write};

#[test]
fn prepared_document_bounds_and_exact_evidence_are_preserved() {
    let id = SaveId::new("story-0123456789abcdef0123456789abcdef").unwrap();
    let previous = Some(ContentStamp::new([1; 32]));
    let stamp = ContentStamp::new([2; 32]);
    assert!(prepare_write(id.clone(), None, stamp, vec![]).is_err());
    assert!(prepare_write(id.clone(), None, stamp, vec![0; MAX_SAVE_BYTES + 1]).is_err());
    let full = prepare_write(id.clone(), previous, stamp, vec![0; MAX_SAVE_BYTES]).unwrap();
    assert_eq!(full.bytes().len(), MAX_SAVE_BYTES);
    let bytes = b"prepared\r\n\xff".to_vec();
    let handle = prepare_write(id.clone(), previous, stamp, bytes.clone()).unwrap();
    assert_eq!(handle.target(), &id);
    assert_eq!(handle.previous_stamp(), previous);
    assert_eq!(handle.intended_stamp(), stamp);
    assert_eq!(handle.bytes(), bytes);
    assert_eq!(handle.clone(), handle);
    assert_eq!(
        handle,
        prepare_write(id.clone(), previous, stamp, bytes).unwrap()
    );
    assert_ne!(
        handle,
        prepare_write(id, previous, stamp, b"other".to_vec()).unwrap()
    );
    assert!(
        !format!("{handle:?}").contains("prepared"),
        "handle Debug must not disclose encoded payload"
    );
}

#[test]
fn foreign_opaque_attempt_cannot_replace_a_real_saved_document() {
    use cyoa_application::cancellation::CancellationSource;
    use cyoa_infrastructure::persistence::{codec, repository::LocalRepository};
    #[derive(PartialEq, Eq)]
    struct Foreign {
        id: SaveId,
        previous: ContentStamp,
        stamp: ContentStamp,
    }
    impl SaveAttempt for Foreign {
        fn target(&self) -> &SaveId {
            &self.id
        }
        fn previous_stamp(&self) -> Option<ContentStamp> {
            Some(self.previous)
        }
        fn intended_stamp(&self) -> ContentStamp {
            self.stamp
        }
    }
    let root = tempfile::tempdir().unwrap();
    let mut repo = LocalRepository::new(root.path().into()).unwrap();
    let token = CancellationSource::default().token();
    let fixture_id = SaveId::new("harbour-0123456789abcdef0123456789abcdef").unwrap();
    let snapshot = codec::decode(
        include_bytes!("fixtures/saves/v1-full-story.json"),
        &fixture_id,
        SaveCopy::Primary,
    )
    .unwrap()
    .snapshot;
    let receipt = repo
        .create(snapshot, &PreparedWriteEvidence::default(), &token)
        .unwrap();
    let path = root
        .path()
        .join("saves")
        .join(format!("{}.json", receipt.metadata.id.as_str()));
    let before = std::fs::read(&path).unwrap();
    let foreign = PendingWrite::new(Foreign {
        id: receipt.metadata.id,
        previous: receipt.stamp,
        stamp: codec::stamp(&[]),
    });
    assert!(
        repo.reconcile(foreign, &PreparedWriteEvidence::default(), &token)
            .is_err()
    );
    assert_eq!(std::fs::read(path).unwrap(), before);
}
