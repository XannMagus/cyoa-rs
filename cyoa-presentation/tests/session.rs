mod support;
use cyoa_application::generation::TurnDirection;
use cyoa_core::{
    game::TurnCount, limits::Limits, style::StoryStyle, text::Brief, world::PlayablePosition,
};
use cyoa_presentation::session::*;
use support::*;

#[test]
fn failed_and_cancelled_results_preserve_state_and_exact_evidence() {
    let mut s = SessionController::from_game(game());
    let before = s.game().unwrap().clone();
    let r = s.take_turn(TurnDirection::Continue).unwrap();
    assert_eq!(
        s.take_turn(TurnDirection::Continue).unwrap_err(),
        SessionError::Busy
    );
    assert_eq!(
        s.rewind(TurnCount::new(1).unwrap()).unwrap_err(),
        SessionError::Busy
    );
    assert_eq!(
        s.complete(Completion {
            key: r.key(),
            outcome: Err(failure())
        }),
        Acceptance::Failed
    );
    assert_eq!(s.game().unwrap(), &before);
    let Some(Failure::Generation(f)) = s.failure() else {
        panic!("failure evidence absent")
    };
    assert_eq!(
        f.raw_response().expect("backend response").as_str(),
        "candidate é\r\n"
    );
    assert_eq!(f.diagnostics().stdout(), [255, 13, 10]);
    let retry = s.retry().unwrap();
    assert_ne!(r.key().id(), retry.key().id());
    assert!(!retry.token().is_cancelled());
    s.progress(retry.key(), "tentative", false);
    s.cancel();
    s.cancel();
    assert!(retry.token().is_cancelled());
    assert_eq!(s.phase(), Phase::Cancelling);
    assert!(s.preview().is_empty());
    assert!(s.retry().is_err());
    assert_eq!(
        s.complete(Completion {
            key: retry.key(),
            outcome: Ok(WorkSuccess::Turn(Box::new(committed(before.clone()))))
        }),
        Acceptance::Failed
    );
    assert_eq!(s.game().unwrap(), &before, "cancelled success regression");
    assert!(matches!(
        s.failure(),
        Some(Failure::Cancelled { rejected: Ok(_) })
    ));
}

#[test]
fn stale_and_duplicate_completions_never_replace_current_state() {
    let mut s = SessionController::from_game(game());
    let old = s.take_turn(TurnDirection::Continue).unwrap();
    s.cancel();
    s.complete(Completion {
        key: old.key(),
        outcome: Err(failure()),
    });
    let fresh = s.retry().unwrap();
    let before = s.game().unwrap().clone();
    assert_eq!(
        s.complete(Completion {
            key: old.key(),
            outcome: Ok(WorkSuccess::Turn(Box::new(committed(before.clone()))))
        }),
        Acceptance::Ignored,
        "stale completion regression"
    );
    assert_eq!(s.phase(), Phase::Running);
    assert_eq!(
        s.complete(Completion {
            key: fresh.key(),
            outcome: Ok(WorkSuccess::Turn(Box::new(committed(before))))
        }),
        Acceptance::Committed
    );
    assert_eq!(
        s.game().unwrap().turns().len(),
        1,
        "double commit regression"
    );
    assert_eq!(
        s.complete(Completion {
            key: fresh.key(),
            outcome: Err(failure())
        }),
        Acceptance::Ignored
    );
    s.progress(old.key(), "obsolete", false);
    assert!(s.preview().is_empty());
    s.cancel();
    assert_eq!(s.phase(), Phase::Ready);
}

#[test]
fn lifecycle_keeps_edited_outline_selection_and_one_opening() {
    let mut s = SessionController::new(Limits::default(), StoryStyle::default());
    assert!(s.accept_outline().is_err());
    let r = s.submit_brief(Brief::new("bell").unwrap()).unwrap();
    assert_eq!(
        s.complete(Completion {
            key: r.key(),
            outcome: Ok(WorkSuccess::Outline(generated(outline("original"))))
        }),
        Acceptance::Committed
    );
    s.replace_outline(outline("edited")).unwrap();
    let r = s.accept_outline().unwrap();
    let mut w = world();
    w = cyoa_core::world::World::new(outline("edited"), w.cast().clone());
    s.complete(Completion {
        key: r.key(),
        outcome: Ok(WorkSuccess::Cast(w)),
    });
    assert_eq!(
        s.select(PlayablePosition::new(99)),
        Err(SessionError::Selection)
    );
    s.select(PlayablePosition::new(1)).unwrap();
    assert_eq!(
        s.game().unwrap().world().outline().title().as_str(),
        "edited"
    );
    let r = s.take_turn(TurnDirection::Continue).unwrap();
    let g = committed(s.game().unwrap().clone());
    assert_eq!(
        s.complete(Completion {
            key: r.key(),
            outcome: Ok(WorkSuccess::Turn(Box::new(g)))
        }),
        Acceptance::Committed
    );
    assert_eq!(s.game().unwrap().turns().len(), 1);
    let rev = s.revision();
    s.rewind(TurnCount::new(1).unwrap()).unwrap();
    assert!(s.revision().get() > rev.get());
    assert!(s.game().unwrap().turns().is_empty());
    s.quit();
    assert_eq!(s.phase(), Phase::Closed);
}

#[test]
fn wrong_kind_faults_and_quit_waits_for_active_completion() {
    let mut s = SessionController::from_game(game());
    let before = s.game().unwrap().clone();
    let r = s.take_turn(TurnDirection::Continue).unwrap();
    assert_eq!(
        s.complete(Completion {
            key: r.key(),
            outcome: Ok(WorkSuccess::Cast(world()))
        }),
        Acceptance::Failed
    );
    assert_eq!(s.phase(), Phase::Faulted);
    assert_eq!(s.game().unwrap(), &before);
    let mut s = SessionController::from_game(before);
    let r = s.take_turn(TurnDirection::Continue).unwrap();
    s.quit();
    assert_eq!(s.phase(), Phase::Closing);
    assert!(r.token().is_cancelled());
    assert_eq!(
        s.complete(Completion {
            key: r.key(),
            outcome: Err(failure())
        }),
        Acceptance::Closed
    );
}

#[test]
fn obsolete_cast_cannot_replace_an_edited_outline_and_actions_are_checked() {
    let mut s = SessionController::new(Limits::default(), StoryStyle::default());
    let r = s.submit_brief(Brief::new("bell").unwrap()).unwrap();
    s.complete(Completion {
        key: r.key(),
        outcome: Ok(WorkSuccess::Outline(generated(outline("first")))),
    });
    let old = s.accept_outline().unwrap();
    s.cancel();
    s.complete(Completion {
        key: old.key(),
        outcome: Err(failure()),
    });
    s.replace_outline(outline("second")).unwrap();
    let fresh = s.accept_outline().unwrap();
    let before = s.stage().clone();
    assert_eq!(
        s.complete(Completion {
            key: old.key(),
            outcome: Ok(WorkSuccess::Cast(world()))
        }),
        Acceptance::Ignored
    );
    assert_eq!(s.stage(), &before);
    assert_eq!(s.phase(), Phase::Running);
    let world = cyoa_core::world::World::new(outline("second"), world().cast().clone());
    s.complete(Completion {
        key: fresh.key(),
        outcome: Ok(WorkSuccess::Cast(world)),
    });
    s.select(PlayablePosition::new(0)).unwrap();
    let r = s.take_turn(TurnDirection::Continue).unwrap();
    s.complete(Completion {
        key: r.key(),
        outcome: Ok(WorkSuccess::Turn(Box::new(committed(
            s.game().unwrap().clone(),
        )))),
    });
    assert_eq!(s.action(0).unwrap_err(), SessionError::Selection);
    assert_eq!(s.action(2).unwrap_err(), SessionError::Selection);
    assert!(!s.action(1).unwrap().token().is_cancelled());
}

#[test]
fn in_session_load_advances_revision_preserves_request_ids_and_rejects_old_generation() {
    let mut s = SessionController::from_game(game());
    let old = s.take_turn(TurnDirection::Continue).unwrap();
    let before = s.stage().clone();
    assert_eq!(s.replace_game(committed(game())), Err(SessionError::Busy));
    assert_eq!(s.stage(), &before);
    s.cancel();
    s.complete(Completion {
        key: old.key(),
        outcome: Err(failure()),
    });
    let revision = s.revision();
    let restored = committed(game());
    s.replace_game(restored.clone()).unwrap();
    assert!(s.revision().get() > revision.get());
    assert_eq!(s.phase(), Phase::Ready);
    assert!(s.failure().is_none());
    assert!(s.preview().is_empty());
    assert_eq!(
        s.complete(Completion {
            key: old.key(),
            outcome: Ok(WorkSuccess::Turn(Box::new(committed(restored.clone()))))
        }),
        Acceptance::Ignored
    );
    assert_eq!(s.game(), Some(&restored));
    let fresh = s.take_turn(TurnDirection::Continue).unwrap();
    assert!(fresh.key().id().get() > old.key().id().get());
}
