use symbio_core::{
    Actor, AppCore, Command, ProjectStore, Request,
    model::{CellInk, InkPoint, Project},
};

fn edit(core: &mut AppCore, command: Command) -> symbio_core::Result<symbio_core::Snapshot> {
    core.apply(Request {
        expected_revision: core.project().revision,
        actor: Actor::Human,
        command,
    })
}

fn ink(x: f32) -> CellInk {
    CellInk {
        strokes: vec![vec![InkPoint { x, y: 20. }, InkPoint { x, y: 80. }]],
    }
}

#[test]
fn ink_round_trips_and_reediting_updates_the_original_cell() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = ProjectStore::open(directory.path().join("ink.symbio")).unwrap();
    let mut core = AppCore::new(Project::empty("Ink").unwrap()).unwrap();
    let inserted = edit(
        &mut core,
        Command::DefineHandwritten {
            id: None,
            name: "ink1".into(),
            source: "1+1".into(),
            ink: Some(ink(40.)),
        },
    )
    .unwrap();
    let id = inserted.cells[0].cell.id;
    assert_eq!(inserted.cells[0].result_text.as_deref(), Some("2"));
    assert!(inserted.cells[0].has_ink);
    assert!(inserted.cells[0].cell.ink.is_none());
    assert!(
        inserted
            .history
            .last()
            .unwrap()
            .command
            .get("ink")
            .is_none()
    );
    store.save(core.project()).unwrap();
    let mut core = AppCore::new(store.load().unwrap().unwrap()).unwrap();
    assert_eq!(
        core.project().current().cells[0]
            .ink
            .as_ref()
            .unwrap()
            .strokes[0][0]
            .x,
        40.
    );
    let updated = edit(
        &mut core,
        Command::DefineHandwritten {
            id: Some(id),
            name: "ink1".into(),
            source: "2+2".into(),
            ink: Some(ink(60.)),
        },
    )
    .unwrap();
    assert_eq!(updated.cells.len(), 1);
    assert_eq!(updated.cells[0].cell.id, id);
    assert_eq!(updated.cells[0].result_text.as_deref(), Some("4"));
    let revision = core.project().revision;
    assert!(
        edit(
            &mut core,
            Command::DefineHandwritten {
                id: Some(id),
                name: "ink1".into(),
                source: "99".into(),
                ink: Some(ink(f32::NAN)),
            }
        )
        .is_err()
    );
    assert_eq!(core.project().revision, revision);
    assert_eq!(core.snapshot().cells[0].result_text.as_deref(), Some("4"));
    edit(&mut core, Command::Undo).unwrap();
    let cell = &core.project().current().cells[0];
    assert_eq!(cell.source, "1+1");
    assert_eq!(cell.ink.as_ref().unwrap().strokes[0][0].x, 40.);
}
